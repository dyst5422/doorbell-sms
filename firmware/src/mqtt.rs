use embassy_net::{dns::DnsQueryType, tcp::TcpSocket, Stack};
use embassy_time::Duration;
use log::{error, info};

use mbedtls_rs::{
    Certificate, ClientSessionConfig, Credentials, PrivateKey, Session, SessionConfig, Tls,
    X509,
};

use rust_mqtt::{
    buffer::BumpBuffer,
    client::{
        Client,
        event::Event,
        options::{ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference},
    },
    types::{MqttString, TopicName},
};

use crate::config::{
    AWS_IOT_ENDPOINT, AWS_IOT_ENDPOINT_CSTR, AWS_IOT_PORT, CA_CERT, DEVICE_CERT, DEVICE_KEY,
    MQTT_CLIENT_ID, TOPIC_RING, TOPIC_SHADOW_GET, TOPIC_SHADOW_GET_ACCEPTED, TOPIC_SHADOW_UPDATE,
};
use crate::doorbell;
use crate::shadow::{self, Mode};

/// Performs the full MQTT-over-TLS workflow:
/// 1. TCP connect to AWS IoT Core (port 8883)
/// 2. TLS handshake with mutual certificate authentication via mbedtls
/// 3. MQTT connect
/// 4. Get Device Shadow (determine mode)
/// 5. Execute mode logic
/// 6. Publish ring event if needed
/// 7. Update reported shadow state
pub async fn mqtt_workflow(
    tls: &mut Tls<'_>,
    stack: Stack<'static>,
    relay_pin: &mut esp_hal::gpio::Output<'_>,
) -> Result<(), ()> {
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));

    // DNS lookup
    info!("[mqtt] Resolving {}...", AWS_IOT_ENDPOINT);
    let address = match stack
        .dns_query(AWS_IOT_ENDPOINT, DnsQueryType::A)
        .await
        .map(|a| a[0])
    {
        Ok(addr) => addr,
        Err(e) => {
            error!("[mqtt] DNS lookup failed: {:?}", e);
            return Err(());
        }
    };

    // TCP connect
    let remote_endpoint = (address, AWS_IOT_PORT);
    info!("[mqtt] Connecting TCP to {:?}...", remote_endpoint);
    if let Err(e) = socket.connect(remote_endpoint).await {
        error!("[mqtt] TCP connect failed: {:?}", e);
        return Err(());
    }
    info!("[mqtt] TCP connected");

    // TLS handshake with mutual authentication via mbedtls-rs
    info!("[mqtt] Starting TLS handshake...");

    let client_conf = ClientSessionConfig {
        ca_chain: Some(Certificate::new(X509::PEM(CA_CERT)).unwrap()),
        server_name: Some(AWS_IOT_ENDPOINT_CSTR),
        creds: Some(Credentials {
            certificate: Certificate::new(X509::PEM(DEVICE_CERT)).unwrap(),
            private_key: PrivateKey::new(X509::PEM(DEVICE_KEY), None).unwrap(),
        }),
        ..ClientSessionConfig::new()
    };

    let mut session = Session::new(tls.reference(), socket, &SessionConfig::Client(client_conf))
        .map_err(|e| {
            error!("[mqtt] TLS session creation failed: {:?}", e);
        })?;

    info!("[mqtt] TLS handshake complete");

    // MQTT client setup over TLS connection using BumpBuffer
    let mut mqtt_buffer = [0u8; 1024];
    let mut buffer = BumpBuffer::new(&mut mqtt_buffer);

    let mut client = Client::<_, _, 1, 1, 1, 1>::new(&mut buffer);

    // Connect to MQTT broker
    info!("[mqtt] Connecting to MQTT broker...");
    let client_id = MqttString::try_from(MQTT_CLIENT_ID).unwrap();
    match client
        .connect(
            &mut session,
            &ConnectOptions::new().clean_start(),
            Some(client_id),
        )
        .await
    {
        Ok(_) => info!("[mqtt] MQTT connected"),
        Err(e) => {
            error!("[mqtt] MQTT connect failed: {:?}", e);
            return Err(());
        }
    }

    // Subscribe to shadow get/accepted
    info!("[mqtt] Subscribing to shadow topic...");

    // Publish debug boot message
    let debug_topic = TopicName::new(MqttString::try_from("doorbell/debug").unwrap()).unwrap();
    let debug_pub_options = PublicationOptions::new(TopicReference::Name(debug_topic));
    let _ = client
        .publish(&debug_pub_options, rust_mqtt::Bytes::from(&b"{\"event\":\"boot\"}"[..]))
        .await;
    info!("[mqtt] Debug boot message published");

    let shadow_topic = TopicName::new(MqttString::try_from(TOPIC_SHADOW_GET_ACCEPTED).unwrap()).unwrap();
    match client
        .subscribe(shadow_topic.clone().into(), SubscriptionOptions::new().at_least_once())
        .await
    {
        Ok(_) => {}
        Err(e) => {
            error!("[mqtt] Subscribe failed: {:?}", e);
            return Err(());
        }
    }

    // Wait for SUBACK
    match client.poll().await {
        Ok(Event::Suback(_)) => info!("[mqtt] Subscribed to shadow topic"),
        Ok(e) => {
            error!("[mqtt] Unexpected event after subscribe: {:?}", e);
            return Err(());
        }
        Err(e) => {
            error!("[mqtt] Poll after subscribe failed: {:?}", e);
            return Err(());
        }
    }

    // Publish to request shadow
    info!("[mqtt] Requesting device shadow...");
    let shadow_get_topic = TopicName::new(MqttString::try_from(TOPIC_SHADOW_GET).unwrap()).unwrap();
    let pub_options = PublicationOptions::new(TopicReference::Name(shadow_get_topic));
    match client
        .publish(&pub_options, rust_mqtt::Bytes::from(&b""[..]))
        .await
    {
        Ok(_) => {}
        Err(e) => {
            error!("[mqtt] Shadow get publish failed: {:?}", e);
            return Err(());
        }
    }

    // Wait for shadow response (incoming PUBLISH)
    let mode = match client.poll().await {
        Ok(Event::Publish(publish)) => {
            let payload = publish.message.as_ref();
            shadow::parse_mode_from_shadow(payload)
        }
        Ok(e) => {
            error!("[mqtt] Expected publish but got: {:?}", e);
            Mode::Sms
        }
        Err(e) => {
            error!("[mqtt] Failed to receive shadow: {:?}", e);
            Mode::Sms // Default to SMS if we can't read shadow
        }
    };

    info!("[mqtt] Current mode: {:?}", mode);

    // Execute mode logic
    let should_publish = doorbell::execute_mode(mode, relay_pin).await;

    // Publish ring event if mode requires it
    if should_publish {
        let timestamp = 0u64; // TODO: Get actual timestamp from RTC
        let payload = doorbell::build_ring_payload(timestamp);
        info!("[mqtt] Publishing ring event...");
        let ring_topic = TopicName::new(MqttString::try_from(TOPIC_RING).unwrap()).unwrap();
        let pub_options = PublicationOptions::new(TopicReference::Name(ring_topic)).at_least_once();
        match client
            .publish(&pub_options, rust_mqtt::Bytes::from(payload.as_bytes()))
            .await
        {
            Ok(_) => info!("[mqtt] Ring event published!"),
            Err(e) => error!("[mqtt] Publish failed: {:?}", e),
        }
    }

    // Update reported shadow state
    let reported = shadow::build_reported_state(mode, 0);
    info!("[mqtt] Updating reported shadow...");
    let shadow_update_topic = TopicName::new(MqttString::try_from(TOPIC_SHADOW_UPDATE).unwrap()).unwrap();
    let pub_options = PublicationOptions::new(TopicReference::Name(shadow_update_topic));
    match client
        .publish(&pub_options, rust_mqtt::Bytes::from(reported.as_bytes()))
        .await
    {
        Ok(_) => info!("[mqtt] Shadow updated"),
        Err(e) => error!("[mqtt] Shadow update failed: {:?}", e),
    }

    info!("[mqtt] Workflow complete");

    // Close TLS session
    let _ = session.close().await;

    Ok(())
}
