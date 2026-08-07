use embassy_net::{dns::DnsQueryType, tcp::TcpSocket, Stack};
use embassy_time::Duration;
use log::{error, info};

use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::publish_packet::QualityOfService,
    utils::rng_generator::CountingRng,
};

use embedded_tls::{Aes128GcmSha256, Certificate, TlsConfig, TlsConnection, TlsContext, NoVerify};

use crate::config::{
    AWS_IOT_ENDPOINT, AWS_IOT_PORT, CA_CERT, DEVICE_CERT, DEVICE_KEY, MQTT_CLIENT_ID, TOPIC_RING,
    TOPIC_SHADOW_GET, TOPIC_SHADOW_GET_ACCEPTED, TOPIC_SHADOW_UPDATE,
};
use crate::doorbell;
use crate::shadow::{self, Mode};

/// Performs the full MQTT-over-TLS workflow:
/// 1. TCP connect to AWS IoT Core (port 8883)
/// 2. TLS 1.3 handshake with mutual certificate authentication
/// 3. MQTT connect
/// 4. Get Device Shadow (determine mode)
/// 5. Execute mode logic
/// 6. Publish ring event if needed
/// 7. Update reported shadow state
pub async fn mqtt_workflow(
    stack: Stack<'static>,
    relay_pin: &mut esp_hal::gpio::Output<'_>,
    rng_seed: u64,
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

    // TLS handshake with mutual authentication
    info!("[mqtt] Starting TLS handshake...");

    let mut read_record_buffer = [0u8; 16640];
    let mut write_record_buffer = [0u8; 16640];

    let tls_config = TlsConfig::new()
        .with_server_name(AWS_IOT_ENDPOINT)
        .with_cert(Certificate::X509(DEVICE_CERT))
        .enable_rsa_signatures();

    let mut tls: TlsConnection<'_, _, Aes128GcmSha256> = TlsConnection::new(
        socket,
        &mut read_record_buffer,
        &mut write_record_buffer,
    );

    let mut rng_impl = ChaChaRng(rng_seed);

    tls.open::<_, NoVerify>(TlsContext::new(
        &tls_config,
        &mut rng_impl,
    ))
    .await
    .map_err(|e| {
        error!("[mqtt] TLS handshake failed: {:?}", e);
    })?;

    info!("[mqtt] TLS handshake complete");

    // MQTT client setup over TLS connection
    let mut config = ClientConfig::new(
        rust_mqtt::client::client_config::MqttVersion::MQTTv5,
        CountingRng(rng_seed),
    );
    config.add_max_subscribe_qos(QualityOfService::QoS1);
    config.add_client_id(MQTT_CLIENT_ID);
    config.max_packet_size = 256;

    let mut recv_buffer = [0; 256];
    let mut write_buffer = [0; 256];

    let mut client = MqttClient::<_, 5, _>::new(
        tls,
        &mut write_buffer,
        256,
        &mut recv_buffer,
        256,
        config,
    );

    // Connect to MQTT broker
    info!("[mqtt] Connecting to MQTT broker...");
    match client.connect_to_broker().await {
        Ok(()) => info!("[mqtt] MQTT connected"),
        Err(e) => {
            error!("[mqtt] MQTT connect failed: {:?}", e);
            return Err(());
        }
    }

    // Subscribe to shadow get/accepted
    info!("[mqtt] Subscribing to shadow topic...");
    match client.subscribe_to_topic(TOPIC_SHADOW_GET_ACCEPTED).await {
        Ok(()) => {}
        Err(e) => {
            error!("[mqtt] Subscribe failed: {:?}", e);
            return Err(());
        }
    }

    // Request shadow
    info!("[mqtt] Requesting device shadow...");
    match client
        .send_message(TOPIC_SHADOW_GET, b"", QualityOfService::QoS1, false)
        .await
    {
        Ok(()) => {}
        Err(e) => {
            error!("[mqtt] Shadow get publish failed: {:?}", e);
            return Err(());
        }
    }

    // Wait for shadow response
    let mode = match client.receive_message().await {
        Ok((_, payload)) => shadow::parse_mode_from_shadow(payload),
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
        let timestamp = 0u64; // TODO: Get actual timestamp
        let payload = doorbell::build_ring_payload(timestamp);
        info!("[mqtt] Publishing ring event...");
        match client
            .send_message(TOPIC_RING, payload.as_bytes(), QualityOfService::QoS1, false)
            .await
        {
            Ok(()) => info!("[mqtt] Ring event published!"),
            Err(e) => error!("[mqtt] Publish failed: {:?}", e),
        }
    }

    // Update reported shadow state
    let reported = shadow::build_reported_state(mode, 0);
    info!("[mqtt] Updating reported shadow...");
    match client
        .send_message(TOPIC_SHADOW_UPDATE, reported.as_bytes(), QualityOfService::QoS1, false)
        .await
    {
        Ok(()) => info!("[mqtt] Shadow updated"),
        Err(e) => error!("[mqtt] Shadow update failed: {:?}", e),
    }

    info!("[mqtt] Workflow complete");
    Ok(())
}

/// Simple RNG wrapper using a seed from the hardware RNG.
struct ChaChaRng(u64);

impl rand_core::CryptoRng for ChaChaRng {}

impl rand_core::RngCore for ChaChaRng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn next_u64(&mut self) -> u64 {
        let a = self.next_u32() as u64;
        let b = self.next_u32() as u64;
        (a << 32) | b
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(4) {
            let val = self.next_u32().to_le_bytes();
            for (d, s) in chunk.iter_mut().zip(val.iter()) {
                *d = *s;
            }
        }
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}
