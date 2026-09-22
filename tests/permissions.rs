//! Verifica que cada usuario de servicio puede hacer **solo** lo que sus
//! permisos declaran en `rabbitmq/definitions.json`: caso positivo (puede
//! publicar/consumir lo suyo, confirmado end-to-end, no solo "no dio
//! error") y caso negativo (acceso denegado a la cola/exchange de otro
//! servicio, `403 ACCESS_REFUSED`) — ver `docs/conventions.md` y
//! `docs/security-scope.md`.

mod common;

use std::time::Duration;

use broker_verification::lab_credentials as creds;
use lapin::options::{BasicGetOptions, BasicPublishOptions};
use lapin::BasicProperties;

const LAB_PAYLOAD: &[u8] = br#"{"note":"lab-only-not-a-real-secret payload"}"#;

async fn publish(channel: &lapin::Channel, exchange: &str, routing_key: &str) {
    channel
        .basic_publish(
            exchange.into(),
            routing_key.into(),
            BasicPublishOptions::default(),
            LAB_PAYLOAD,
            BasicProperties::default(),
        )
        .await
        .unwrap_or_else(|err| panic!("publicar en {exchange} debe aceptarse: {err}"))
        .await
        .unwrap_or_else(|err| panic!("el broker debe confirmar la entrega a {exchange}: {err}"));
}

/// Sondea `basic.get` sobre `queue` hasta recibir un mensaje (los tests
/// que la usan la envuelven en un `tokio::time::timeout`).
async fn wait_for_message(channel: &lapin::Channel, queue: &str) -> Vec<u8> {
    loop {
        if let Some(message) = channel
            .basic_get(queue.into(), BasicGetOptions::default())
            .await
            .expect("basic.get no debe fallar para un usuario con permiso de lectura")
        {
            return message.delivery.data;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Un `basic.get` sobre un recurso fuera del permiso `read` del usuario
/// debe fallar con un *soft error* AMQP (403 ACCESS_REFUSED), cerrando el
/// canal — nunca la conexión completa. `basic.get` es una RPC síncrona:
/// el error llega en el primer `.await`, sin ambigüedad de confirmación
/// asíncrona (ver `progress/explore_lapin_testcontainers.md` §2).
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

/// Un `basic.publish` seguido de una RPC síncrona en el mismo canal
/// (`basic.get` sobre una cola cualquiera) debe fallar con un *soft
/// error*: si el publish fue rechazado por permisos, el broker ya cerró
/// el canal antes de que llegue esta segunda operación.
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
            LAB_PAYLOAD,
            BasicProperties::default(),
        )
        .await;
    // Forzamos una RPC síncrona sobre el mismo canal: si el publish violó
    // permisos, el canal ya está cerrado y esta operación falla con el
    // soft error 403 que confirma el rechazo.
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
async fn gateway_can_write_scan_requests_and_read_own_outcomes() {
    let container = common::start_broker().await;

    // POSITIVO — escritura: gateway publica en scan.requests y el mensaje
    // llega íntegro a ms-nmap.scan-requests (confirmado por un admin, que
    // sí tiene permiso para leerla).
    let gateway =
        common::connect_as(&container, creds::GATEWAY_USER, creds::GATEWAY_PASSWORD).await;
    let gateway_channel = gateway.create_channel().await.expect("canal de gateway");
    publish(&gateway_channel, "scan.requests", "scan.request").await;

    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let admin_channel = admin.create_channel().await.expect("canal de admin");
    let delivered = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_message(&admin_channel, "ms-nmap.scan-requests"),
    )
    .await
    .expect("no debe hacer timeout esperando el mensaje de gateway");
    assert_eq!(delivered, LAB_PAYLOAD);

    // POSITIVO — lectura: un mensaje publicado (por el admin) en
    // scan.outcomes con routing key scan.outcome.completed debe poder
    // leerlo gateway desde su propia cola.
    publish(&admin_channel, "scan.outcomes", "scan.outcome.completed").await;

    let received = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_message(&gateway_channel, "gateway.scan-outcomes"),
    )
    .await
    .expect("gateway debe poder leer su propia cola gateway.scan-outcomes");
    assert_eq!(received, LAB_PAYLOAD);
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn gateway_cannot_read_ms_nmap_or_ms_analisis_queues() {
    let container = common::start_broker().await;
    let gateway =
        common::connect_as(&container, creds::GATEWAY_USER, creds::GATEWAY_PASSWORD).await;

    // NEGATIVO: gateway no tiene permiso de lectura sobre la cola de
    // ms-nmap ni sobre la de ms-analisis.
    assert_read_denied(&gateway, "ms-nmap.scan-requests").await;
    assert_read_denied(&gateway, "ms-analisis.scan-outcomes").await;
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn ms_nmap_can_read_own_queue_and_publish_scan_outcomes() {
    let container = common::start_broker().await;

    // POSITIVO — lectura: un mensaje publicado por el admin en
    // scan.requests llega a la cola de ms-nmap y ms-nmap puede leerlo.
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let admin_channel = admin.create_channel().await.expect("canal de admin");
    publish(&admin_channel, "scan.requests", "scan.request").await;

    let ms_nmap =
        common::connect_as(&container, creds::MS_NMAP_USER, creds::MS_NMAP_PASSWORD).await;
    let ms_nmap_channel = ms_nmap.create_channel().await.expect("canal de ms-nmap");
    let received = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_message(&ms_nmap_channel, "ms-nmap.scan-requests"),
    )
    .await
    .expect("ms-nmap debe poder leer su propia cola");
    assert_eq!(received, LAB_PAYLOAD);

    // POSITIVO — escritura: ms-nmap publica en scan.outcomes y llega tanto
    // a gateway.scan-outcomes como (si es completed/failed) a
    // ms-analisis.scan-outcomes; aquí basta confirmar que llega a una.
    publish(&ms_nmap_channel, "scan.outcomes", "scan.outcome.completed").await;

    let received = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_message(&admin_channel, "gateway.scan-outcomes"),
    )
    .await
    .expect("el desenlace publicado por ms-nmap debe llegar a gateway.scan-outcomes");
    assert_eq!(received, LAB_PAYLOAD);
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn ms_nmap_cannot_write_scan_requests_or_read_other_queues() {
    let container = common::start_broker().await;
    let ms_nmap =
        common::connect_as(&container, creds::MS_NMAP_USER, creds::MS_NMAP_PASSWORD).await;

    // NEGATIVO: ms-nmap no tiene permiso de escritura sobre scan.requests
    // (solo lo consume) ni de lectura sobre las colas de otros servicios.
    assert_write_denied(&ms_nmap, "scan.requests", "scan.request").await;
    assert_read_denied(&ms_nmap, "gateway.scan-outcomes").await;
    assert_read_denied(&ms_nmap, "ms-analisis.scan-outcomes").await;
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn ms_analisis_can_read_own_queue_only() {
    let container = common::start_broker().await;

    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let admin_channel = admin.create_channel().await.expect("canal de admin");
    publish(&admin_channel, "scan.outcomes", "scan.outcome.failed").await;

    let ms_analisis = common::connect_as(
        &container,
        creds::MS_ANALISIS_USER,
        creds::MS_ANALISIS_PASSWORD,
    )
    .await;
    let ms_analisis_channel = ms_analisis
        .create_channel()
        .await
        .expect("canal de ms-analisis");
    let received = tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_message(&ms_analisis_channel, "ms-analisis.scan-outcomes"),
    )
    .await
    .expect("ms-analisis debe poder leer su propia cola");
    assert_eq!(received, LAB_PAYLOAD);
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn ms_analisis_cannot_write_anything_or_read_other_queues() {
    let container = common::start_broker().await;
    let ms_analisis = common::connect_as(
        &container,
        creds::MS_ANALISIS_USER,
        creds::MS_ANALISIS_PASSWORD,
    )
    .await;

    // NEGATIVO: ms-analisis solo lee su propia cola — nada de escritura,
    // ni lectura de las colas de gateway o ms-nmap.
    assert_write_denied(&ms_analisis, "scan.outcomes", "scan.outcome.completed").await;
    assert_write_denied(&ms_analisis, "scan.requests", "scan.request").await;
    assert_read_denied(&ms_analisis, "gateway.scan-outcomes").await;
    assert_read_denied(&ms_analisis, "ms-nmap.scan-requests").await;
}
