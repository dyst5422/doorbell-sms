use embassy_net::{dns::DnsQueryType, tcp::TcpSocket, Stack};
use embassy_time::{Duration, Timer};
use log::{error, info};

use rust_mqtt::{
    client::{client::MqttClient, client_config::ClientConfig},
    packet::v5::{publish_packet::QualityOfService, reason_codes::ReasonCode},
    utils::rng_generator::CountingRng,
};

use crate::config::{AWS_IOT_ENDPOINT, AWS_IOT_PORT, MQTT_CLIENT_ID, TOPIC_RING, TOPIC_SHADOW_GET, TOPIC_SHADOW_GET_ACCEPTED, TOPIC_SHADOW_UPDATE};
use crate::shadow::{self, Mode};
use crate::doorbell;

/// Performs the full MQTT workflow:
/// 1. Connect to AWS IoT Core
/// 2. Get Device Shadow (determine mode)
/// 3. Execute mode logic
/// 4. Publish ring event if needed
/// 5. Update reported shadow state
///
/// Note: TLS is not yet implemented in this scaffold.
/// For initial testing, this connects without TLS on port 1883.
/// Production will use embedded-tls on port 8883 with mutual certificate auth.
pub async fn mqtt_workflow(
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
    // TODO: Wrap with embedded-tls for mutual TLS auth
    let remote_endpoint = (address, AWS_IOT_PORT);
    info!("[mqtt] Connecting to {:?}...", remote_endpoint);
    if let Err(e) = socket.connect(remote_endpoint).await {
        error!("[mqtt] TCP connect failed: {:?}", e);
        return Err(());
    }
    info!("[mqtt] TCP connected");

    // MQTT client setup
    let mut config = ClientConfig::new(
        rust_mqtt::client::client_config::MqttVersion::MQTTv5,
        CountingRng(12345),
    );
    config.add_max_subscribe_qos(QualityOfService::QoS1);
    config.add_client_id(MQTT_CLIENT_ID);
    config.max_packet_size = 256;

    let mut recv_buffer = [0; 256];
    let mut write_buffer = [0; 256];

    let mut client = MqttClient::<_, 5, _>::new(
        socket,
        &mut write_buffer,
        256,
        &mut recv_buffer,
        256,
        config,
    );

    // Connect to broker
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
    match client
        .subscribe_to_topic(TOPIC_SHADOW_GET_ACCEPTED)
        .await
    {
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
    // TODO: Add timeout handling
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
        let timestamp = 0u64; // TODO: Get actual timestamp from SNTP or shadow metadata
        let payload = doorbell::build_ring_payload(timestamp);
        info!("[mqtt] Publishing ring event...");
        match client
            .send_message(
                TOPIC_RING,
                payload.as_bytes(),
                QualityOfService::QoS1,
                false,
            )
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
        .send_message(
            TOPIC_SHADOW_UPDATE,
            reported.as_bytes(),
            QualityOfService::QoS1,
            false,
        )
        .await
    {
        Ok(()) => info!("[mqtt] Shadow updated"),
        Err(e) => error!("[mqtt] Shadow update failed: {:?}", e),
    }

    info!("[mqtt] Workflow complete");
    Ok(())
}
