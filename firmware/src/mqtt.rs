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
    MQTT_CLIENT_ID, TOPIC_RING, TOPIC_SHADOW_UPDATE,
};
use crate::doorbell;
use crate::shadow::{self, Mode};

/// MQTT workflow:
/// 1. Connect (WiFi already done) → TLS → MQTT
/// 2. Immediately publish doorbell/ring (server decides whether to SMS)
/// 3. Subscribe to doorbell/config, read retained mode
/// 4. If mode includes chime → fire relay
/// 5. Publish timing debug, sleep
pub async fn mqtt_workflow(
    tls: &mut Tls<'_>,
    stack: Stack<'static>,
    relay_pin: &mut esp_hal::gpio::Output<'_>,
    wake_start: embassy_time::Instant,
    battery_mv: u32,
) -> Result<(), ()> {
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
    let tls_ms = embassy_time::Instant::now().duration_since(mqtt_start).as_millis();
    info!("[timing] TLS complete: {}ms", tls_ms);

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

    // === STEP 1: Publish ring event IMMEDIATELY ===
    // Server-side Lambda will check mode before sending SMS
    let ring_ms = embassy_time::Instant::now().duration_since(wake_start).as_millis();
    let mut ring_payload: heapless::String<128> = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(
        &mut ring_payload,
        format_args!(
            r#"{{"event":"ring","ring_ms":{},"device":"doorbell"}}"#,
            ring_ms
        ),
    );
    info!("[mqtt] Publishing ring event...");
    let ring_topic = TopicName::new(MqttString::try_from(TOPIC_RING).unwrap()).unwrap();
    let pub_options = PublicationOptions::new(TopicReference::Name(ring_topic)).at_least_once();
    match client
        .publish(&pub_options, rust_mqtt::Bytes::from(ring_payload.as_bytes()))
        .await
    {
        Ok(_) => {
            info!("[mqtt] Ring event published! ({}ms)", ring_ms);
        }
        Err(e) => error!("[mqtt] Ring publish failed: {:?}", e),
    }

    // === STEP 2: Read config for chime decision ===
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
    let mut mode = Mode::Sms;
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

    let config_ms = embassy_time::Instant::now().duration_since(wake_start).as_millis();
    info!("[mqtt] Mode: {:?} (config at {}ms from wake)", mode, config_ms);

    // === STEP 3: Fire chime if mode requires it ===
    if mode.should_ring_chime() {
        doorbell::execute_mode(mode, relay_pin).await;
    }
    let chime_ms = embassy_time::Instant::now().duration_since(wake_start).as_millis();

    // === STEP 4: Publish timing debug with chime_ms ===
    let total_mqtt_ms = embassy_time::Instant::now().duration_since(mqtt_start).as_millis();

    let mut timing_payload: heapless::String<256> = heapless::String::new();
    let _ = core::fmt::Write::write_fmt(
        &mut timing_payload,
        format_args!(
            r#"{{"tls_ms":{},"ring_ms":{},"chime_ms":{},"config_ms":{},"total_ms":{},"battery_mv":{}}}"#,
            tls_ms, ring_ms, chime_ms, config_ms, total_mqtt_ms, battery_mv
        ),
    );
    let timing_topic = TopicName::new(MqttString::try_from("doorbell/debug").unwrap()).unwrap();
    let timing_pub_options = PublicationOptions::new(TopicReference::Name(timing_topic));
    let _ = client
        .publish(&timing_pub_options, rust_mqtt::Bytes::from(timing_payload.as_bytes()))
        .await;

    info!("[mqtt] Workflow complete ({}ms)", total_mqtt_ms);

    // Close TLS session
    let _ = session.close().await;

    Ok(())
}
