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
    MQTT_CLIENT_ID,
};
use crate::shadow::{self, Mode};

/// Simplified MQTT workflow:
/// 1. Connect (WiFi already done) → TLS → MQTT
/// 2. Subscribe to doorbell/config, read retained mode
/// 3. Update device shadow with battery level
/// 4. Return the mode for chime decision
pub async fn mqtt_workflow(
    tls: &mut Tls<'_>,
    stack: Stack<'static>,
    battery_mv: u32,
) -> Result<Mode, ()> {
    let mqtt_start = embassy_time::Instant::now();

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

    // TLS handshake
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

    // MQTT connect
    let mut mqtt_buffer = [0u8; 1024];
    let mut buffer = BumpBuffer::new(&mut mqtt_buffer);
    let mut client = Client::<_, _, 1, 1, 1, 1>::new(&mut buffer);

    info!("[mqtt] Connecting to MQTT broker...");
    let client_id = MqttString::try_from(MQTT_CLIENT_ID).unwrap();
    match client
        .connect(&mut session, &ConnectOptions::new(), Some(client_id))
        .await
    {
        Ok(_) => info!("[mqtt] MQTT connected"),
        Err(e) => {
            error!("[mqtt] MQTT connect failed: {:?}", e);
            return Err(());
        }
    }

    // Subscribe to config topic and read retained message
    info!("[mqtt] Subscribing to config...");
    let config_topic = TopicName::new(MqttString::try_from("doorbell/config").unwrap()).unwrap();
    match client
        .subscribe(config_topic.clone().into(), SubscriptionOptions::new().at_least_once())
        .await
    {
        Ok(_) => {}
        Err(e) => {
            error!("[mqtt] Subscribe failed: {:?}", e);
            return Err(());
        }
    }

    // Poll for SUBACK + retained config
    let mut mode = Mode::On; // Default to chime on
    for _ in 0..5 {
        match client.poll().await {
            Ok(Event::Suback(_)) => {
                info!("[mqtt] Subscribed to config");
            }
            Ok(Event::Publish(publish)) => {
                let payload = publish.message.as_ref();
                mode = shadow::parse_mode_from_config(payload);
                break;
            }
            Ok(_) => {}
            Err(e) => {
                error!("[mqtt] Poll failed: {:?}", e);
                break;
            }
        }
    }

    info!("[mqtt] Mode: {:?}", mode);

    // Update device shadow with battery level
    let mut shadow_payload: heapless::String<128> = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(
        &mut shadow_payload,
        format_args!(
            r#"{{"state":{{"reported":{{"battery_mv":{}}}}}}}"#,
            battery_mv
        ),
    );
    let shadow_topic = TopicName::new(MqttString::try_from("$aws/things/doorbell/shadow/update").unwrap()).unwrap();
    let pub_options = PublicationOptions::new(TopicReference::Name(shadow_topic));
    let _ = client
        .publish(&pub_options, rust_mqtt::Bytes::from(shadow_payload.as_bytes()))
        .await;
    info!("[mqtt] Shadow updated (battery_mv: {})", battery_mv);

    // Publish debug info
    let total_ms = embassy_time::Instant::now().duration_since(mqtt_start).as_millis();
    let mut debug_payload: heapless::String<128> = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(
        &mut debug_payload,
        format_args!(
            r#"{{"battery_mv":{},"total_ms":{}}}"#,
            battery_mv, total_ms
        ),
    );
    let debug_topic = TopicName::new(MqttString::try_from("doorbell/debug").unwrap()).unwrap();
    let debug_pub_options = PublicationOptions::new(TopicReference::Name(debug_topic));
    let _ = client
        .publish(&debug_pub_options, rust_mqtt::Bytes::from(debug_payload.as_bytes()))
        .await;

    info!("[mqtt] Workflow complete ({}ms)", total_ms);

    // Close TLS session
    let _ = session.close().await;

    Ok(mode)
}
