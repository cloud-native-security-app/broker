//! Verifica el enrutamiento diferenciado de `scan.outcomes` (RF-07/RF-08,
//! ver `docs/architecture.md`): `scan.outcome.started` solo le interesa al
//! Gateway (para reflejar EN_PROGRESO), nunca a `ms-analisis`;
//! `scan.outcome.completed` (y `failed`, por el mismo binding `#`/exacto)
//! llega a ambos.

mod common;

use std::time::Duration;

use broker_verification::lab_credentials as creds;
use lapin::options::{BasicGetOptions, BasicPublishOptions};
use lapin::BasicProperties;

const STARTED_PAYLOAD: &[u8] = br#"{"status":"started","correlation_id":"lab-only-req-1"}"#;
const COMPLETED_PAYLOAD: &[u8] = br#"{"status":"completed","correlation_id":"lab-only-req-2"}"#;

async fn publish(channel: &lapin::Channel, routing_key: &str, payload: &'static [u8]) {
    channel
        .basic_publish(
            "scan.outcomes".into(),
            routing_key.into(),
            BasicPublishOptions::default(),
            payload,
            BasicProperties::default(),
        )
        .await
        .expect("publicar en scan.outcomes debe aceptarse")
        .await
        .expect("el broker debe confirmar la entrega");
}

/// Intenta un `basic.get`; `None` si la cola sigue vacía transcurrido el
/// margen dado (a diferencia de `wait_for_message` de `tests/permissions.rs`,
/// aquí un `None` persistente es el resultado *esperado* para el caso
/// negativo, así que no se usa un `loop` infinito).
async fn try_get_message(channel: &lapin::Channel, queue: &str) -> Option<Vec<u8>> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        if let Some(message) = channel
            .basic_get(queue.into(), BasicGetOptions::default())
            .await
            .expect("basic.get no debe fallar para el admin")
        {
            return Some(message.delivery.data);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn started_reaches_only_gateway_queue() {
    let container = common::start_broker().await;
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let channel = admin.create_channel().await.expect("canal de admin");

    publish(&channel, "scan.outcome.started", STARTED_PAYLOAD).await;

    let in_gateway = try_get_message(&channel, "gateway.scan-outcomes").await;
    assert_eq!(
        in_gateway.as_deref(),
        Some(STARTED_PAYLOAD),
        "scan.outcome.started debe llegar a gateway.scan-outcomes (RF-08)"
    );

    let in_ms_analisis = try_get_message(&channel, "ms-analisis.scan-outcomes").await;
    assert_eq!(
        in_ms_analisis, None,
        "scan.outcome.started NO debe llegar a ms-analisis.scan-outcomes"
    );
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn completed_reaches_both_queues() {
    let container = common::start_broker().await;
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let channel = admin.create_channel().await.expect("canal de admin");

    publish(&channel, "scan.outcome.completed", COMPLETED_PAYLOAD).await;

    let in_gateway = try_get_message(&channel, "gateway.scan-outcomes").await;
    assert_eq!(
        in_gateway.as_deref(),
        Some(COMPLETED_PAYLOAD),
        "scan.outcome.completed debe llegar a gateway.scan-outcomes"
    );

    let in_ms_analisis = try_get_message(&channel, "ms-analisis.scan-outcomes").await;
    assert_eq!(
        in_ms_analisis.as_deref(),
        Some(COMPLETED_PAYLOAD),
        "scan.outcome.completed debe llegar también a ms-analisis.scan-outcomes"
    );
}
