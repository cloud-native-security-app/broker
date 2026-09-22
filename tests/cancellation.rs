//! Verifica el canal de cancelación de un escaneo activo (RF-14, feature
//! `cancellation_contract`): el usuario `gateway` puede publicar un
//! `ScanCancellation` válido en `scan.cancellations` y llega íntegro a
//! `ms-nmap.scan-cancellations`; ningún otro usuario de servicio
//! (`ms-analisis`) tiene permiso de escritura ni de lectura sobre este
//! exchange/cola — mismo patrón positivo/negativo que
//! `tests/permissions.rs` (feature `topology_definition`).
//!
//! El payload de ejemplo además valida contra
//! `contracts/scan-cancellation.schema.json` (vía
//! `broker_verification::contracts`), para que este test confirme forma +
//! enrutamiento + permisos, no solo uno de los tres.

mod common;

use std::time::Duration;

use broker_verification::contracts::validate_scan_cancellation;
use broker_verification::lab_credentials as creds;
use lapin::options::{BasicGetOptions, BasicPublishOptions};
use lapin::BasicProperties;
use serde_json::json;

async fn publish(channel: &lapin::Channel, exchange: &str, routing_key: &str, payload: &[u8]) {
    channel
        .basic_publish(
            exchange.into(),
            routing_key.into(),
            BasicPublishOptions::default(),
            payload,
            BasicProperties::default(),
        )
        .await
        .unwrap_or_else(|err| panic!("publicar en {exchange} debe aceptarse: {err}"))
        .await
        .unwrap_or_else(|err| panic!("el broker debe confirmar la entrega a {exchange}: {err}"));
}

/// Espera hasta 2s a que aparezca un mensaje en `queue` (el enrutamiento
/// puede tardar unos ms, igual que en `tests/message_contract.rs`).
async fn wait_for_message(channel: &lapin::Channel, queue: &str) -> Option<Vec<u8>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Some(message) = channel
            .basic_get(queue.into(), BasicGetOptions::default())
            .await
            .expect("basic.get no debe fallar para un usuario con permiso de lectura")
        {
            return Some(message.delivery.data);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

/// Ver `tests/permissions.rs::assert_read_denied` — mismo patrón: un
/// `basic.get` fuera del permiso `read` del usuario debe fallar con un
/// *soft error* AMQP (403 ACCESS_REFUSED).
async fn assert_read_denied(connection: &lapin::Connection, queue: &str) {
    let channel = connection
        .create_channel()
        .await
        .expect("abrir un canal nuevo siempre debe funcionar");
    let result = channel
        .basic_get(queue.into(), BasicGetOptions::default())
        .await;
    match result {
        Err(err) if err.is_amqp_soft_error() => {}
        other => panic!("se esperaba 403 ACCESS_REFUSED leyendo {queue}, llegó: {other:?}"),
    }
}

/// Ver `tests/permissions.rs::assert_write_denied` — mismo patrón: un
/// `basic.publish` rechazado por permisos cierra el canal, confirmado por
/// una segunda RPC síncrona en el mismo canal.
async fn assert_write_denied(connection: &lapin::Connection, exchange: &str, routing_key: &str) {
    let channel = connection
        .create_channel()
        .await
        .expect("abrir un canal nuevo siempre debe funcionar");
    let _ = channel
        .basic_publish(
            exchange.into(),
            routing_key.into(),
            BasicPublishOptions::default(),
            b"{}",
            BasicProperties::default(),
        )
        .await;
    let result = channel
        .basic_get("nonexistent-probe-queue".into(), BasicGetOptions::default())
        .await;
    match result {
        Err(err) if err.is_amqp_soft_error() => {}
        other => panic!(
            "se esperaba que publicar en {exchange} quedara rechazado (403), llegó: {other:?}"
        ),
    }
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn gateway_can_publish_cancellation_and_it_arrives_at_ms_nmap_queue() {
    let payload = json!({
        "correlation_id": "req-2026-0042",
        "requested_by": "analyst@example.test"
    });
    validate_scan_cancellation(&payload).expect("el payload de ejemplo debe cumplir el schema");
    let bytes = serde_json::to_vec(&payload).expect("el payload debe serializar a JSON");

    let container = common::start_broker().await;
    let gateway =
        common::connect_as(&container, creds::GATEWAY_USER, creds::GATEWAY_PASSWORD).await;
    let gateway_channel = gateway.create_channel().await.expect("canal de gateway");

    publish(
        &gateway_channel,
        "scan.cancellations",
        "scan.cancellation",
        &bytes,
    )
    .await;

    // La entrega se confirma como admin (no como gateway): gateway solo
    // tiene permiso de escritura sobre scan.cancellations, nunca de
    // lectura sobre la cola de ms-nmap — mismo patrón que
    // `tests/permissions.rs::gateway_can_write_scan_requests_and_read_own_outcomes`.
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let admin_channel = admin.create_channel().await.expect("canal de admin");
    let received = wait_for_message(&admin_channel, "ms-nmap.scan-cancellations")
        .await
        .expect("el ScanCancellation debe llegar a ms-nmap.scan-cancellations");
    let received_value: serde_json::Value =
        serde_json::from_slice(&received).expect("el mensaje recibido debe ser JSON válido");

    assert_eq!(
        received_value, payload,
        "el mensaje debe llegar íntegro (sin transformaciones) a la cola de ms-nmap"
    );
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn ms_analisis_cannot_write_or_read_cancellation_channel() {
    let container = common::start_broker().await;
    let ms_analisis = common::connect_as(
        &container,
        creds::MS_ANALISIS_USER,
        creds::MS_ANALISIS_PASSWORD,
    )
    .await;

    // NEGATIVO: ms-analisis no forma parte del canal de cancelación — ni
    // puede publicar en scan.cancellations ni leer ms-nmap.scan-cancellations.
    assert_write_denied(&ms_analisis, "scan.cancellations", "scan.cancellation").await;
    assert_read_denied(&ms_analisis, "ms-nmap.scan-cancellations").await;
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn ms_nmap_cannot_write_to_cancellation_exchange() {
    let container = common::start_broker().await;
    let ms_nmap =
        common::connect_as(&container, creds::MS_NMAP_USER, creds::MS_NMAP_PASSWORD).await;

    // NEGATIVO: ms-nmap solo consume la cancelación, no la publica — su
    // permiso de write sigue acotado a scan.outcomes (sin ampliar).
    assert_write_denied(&ms_nmap, "scan.cancellations", "scan.cancellation").await;
}
