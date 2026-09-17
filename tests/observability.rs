//! Verifica RNF-09/RNF-10 (feature `observability`): el estado del nodo y
//! las métricas de cola (mensajes listos, consumidores activos, mensajes
//! en `.dlq`) se pueden consultar vía la API HTTP de management — ver
//! `rabbitmq/README.md` §"Observabilidad". Ningún test de este archivo lee
//! el cuerpo de un mensaje real: solo conteos/metadatos (ver
//! `docs/security-scope.md` §"Cobertura de las features añadidas en la
//! ronda 2").
//!
//! El tercer test reutiliza el mismo mecanismo que
//! `tests/retry_delivery_limit.rs` (publicar + `basic.reject` ×3 hasta
//! agotar `x-delivery-limit`), pero verifica el resultado consultando la
//! API de management en vez de leer la `.dlq` por AMQP: lo que se prueba
//! aquí es que la propia API refleja el mensaje muerto, no el mecanismo de
//! dead-lettering en sí (ya cubierto por `retry_delivery_limit.rs`).

mod common;

use std::time::Duration;

use broker_verification::lab_credentials as creds;
use broker_verification::VHOST;
use lapin::message::Delivery;
use lapin::options::{BasicGetOptions, BasicPublishOptions, BasicRejectOptions};
use lapin::BasicProperties;
use serde_json::Value;

const LAB_PAYLOAD: &[u8] = br#"{"correlation_id":"lab-only-observability-test"}"#;
const MAX_REDELIVERIES: u32 = 10;

/// Las estadísticas de la API de management no son instantáneas: se
/// verificó empíricamente contra un contenedor real
/// (`rabbitmq:4.3.5-management`) que una publicación reciente puede tardar
/// hasta un par de segundos en reflejarse en `messages_ready` — así que
/// los tests de este archivo sondean en vez de asumir una única lectura
/// inmediata.
const HTTP_POLL_TIMEOUT: Duration = Duration::from_secs(15);
const HTTP_POLL_INTERVAL: Duration = Duration::from_millis(500);
const AMQP_POLL_TIMEOUT: Duration = Duration::from_secs(3);
const AMQP_POLL_INTERVAL: Duration = Duration::from_millis(100);

async fn get_json(client: &reqwest::Client, base: &str, path: &str) -> Value {
    client
        .get(format!("{base}{path}"))
        .basic_auth(creds::ADMIN_USER, Some(creds::ADMIN_PASSWORD))
        .send()
        .await
        .expect("la API de management debe responder")
        .error_for_status()
        .expect("la API de management no debe devolver error")
        .json()
        .await
        .expect("la respuesta debe ser JSON válido")
}

/// Sondea `GET /api/queues/<vhost>/<queue>` hasta que `messages_ready`
/// coincida con `expected`, o hasta agotar `HTTP_POLL_TIMEOUT`. Devuelve el
/// último snapshot recibido (para inspeccionar otros campos, p. ej.
/// `consumers`, o para un mensaje de fallo con contexto).
async fn wait_for_messages_ready(
    client: &reqwest::Client,
    base: &str,
    queue: &str,
    expected: i64,
) -> Value {
    let deadline = tokio::time::Instant::now() + HTTP_POLL_TIMEOUT;
    loop {
        let snapshot = get_json(client, base, &format!("/queues/{VHOST}/{queue}")).await;
        let ready = snapshot["messages_ready"].as_i64().unwrap_or(-1);
        if ready == expected {
            return snapshot;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "messages_ready de {queue} no llegó a {expected} tras {HTTP_POLL_TIMEOUT:?} \
                 (API de management), último snapshot: {snapshot}"
            );
        }
        tokio::time::sleep(HTTP_POLL_INTERVAL).await;
    }
}

/// Sondea `basic.get` sobre `queue` hasta obtener un mensaje o agotar
/// `AMQP_POLL_TIMEOUT` — mismo motivo que en `tests/retry_delivery_limit.rs`:
/// justo tras el arranque del contenedor o tras un `basic.reject`, una cola
/// quorum puede tardar un instante en reflejar el nuevo estado.
async fn poll_get(channel: &lapin::Channel, queue: &str) -> Option<Delivery> {
    let deadline = tokio::time::Instant::now() + AMQP_POLL_TIMEOUT;
    loop {
        if let Some(message) = channel
            .basic_get(queue.into(), BasicGetOptions::default())
            .await
            .expect("basic.get no debe fallar para el admin")
        {
            return Some(message.delivery);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(AMQP_POLL_INTERVAL).await;
    }
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn node_healthcheck_reports_ok() {
    let container = common::start_broker().await;
    let base = common::management_base_url(&container).await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/healthchecks/node"))
        .basic_auth(creds::ADMIN_USER, Some(creds::ADMIN_PASSWORD))
        .send()
        .await
        .expect("la API de management debe responder al healthcheck del nodo");

    assert_eq!(
        response.status(),
        reqwest::StatusCode::OK,
        "el healthcheck del nodo debe responder 200 OK (RNF-09)"
    );

    let body: Value = response
        .json()
        .await
        .expect("la respuesta del healthcheck debe ser JSON válido");
    assert_eq!(
        body["status"], "ok",
        "el healthcheck del nodo debe reportar status=ok, respuesta completa: {body}"
    );
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn publishing_n_messages_reports_matching_queue_depth() {
    let container = common::start_broker().await;
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let channel = admin.create_channel().await.expect("canal de admin");
    let base = common::management_base_url(&container).await;
    let client = reqwest::Client::new();

    const N: i64 = 3;
    for i in 0..N {
        channel
            .basic_publish(
                "scan.requests".into(),
                "scan.request".into(),
                BasicPublishOptions::default(),
                LAB_PAYLOAD,
                BasicProperties::default(),
            )
            .await
            .unwrap_or_else(|err| panic!("publicar el mensaje {i} debe aceptarse: {err}"))
            .await
            .unwrap_or_else(|err| {
                panic!("el broker debe confirmar la entrega del mensaje {i}: {err}")
            });
    }

    let snapshot = wait_for_messages_ready(&client, &base, "ms-nmap.scan-requests", N).await;
    assert_eq!(
        snapshot["consumers"], 0,
        "no hay ningún consumidor activo en este test, la API debe reportarlo: {snapshot}"
    );
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn exhausted_message_visible_in_dlq_via_management_api() {
    let container = common::start_broker().await;
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let channel = admin.create_channel().await.expect("canal de admin");
    let base = common::management_base_url(&container).await;
    let client = reqwest::Client::new();

    channel
        .basic_publish(
            "scan.requests".into(),
            "scan.request".into(),
            BasicPublishOptions::default(),
            LAB_PAYLOAD,
            BasicProperties::default(),
        )
        .await
        .expect("publicar en scan.requests debe aceptarse")
        .await
        .expect("el broker debe confirmar la entrega");

    // Agota x-delivery-limit (3) rechazando la misma redelivery repetidas
    // veces, igual mecanismo que tests/retry_delivery_limit.rs.
    let mut redeliveries = 0u32;
    loop {
        let Some(delivery) = poll_get(&channel, "ms-nmap.scan-requests").await else {
            break;
        };
        redeliveries += 1;
        assert!(
            redeliveries <= MAX_REDELIVERIES,
            "el mensaje debería haberse agotado mucho antes de {MAX_REDELIVERIES} entregas \
             (x-delivery-limit=3)"
        );
        channel
            .basic_reject(delivery.delivery_tag, BasicRejectOptions { requeue: true })
            .await
            .expect("basic.reject(requeue=true) debe aceptarse");
    }
    assert!(
        redeliveries > 1,
        "el mensaje debe haberse reentregado más de una vez antes de agotar x-delivery-limit=3, \
         se contaron {redeliveries} entrega(s)"
    );

    let dlq_snapshot =
        wait_for_messages_ready(&client, &base, "ms-nmap.scan-requests.dlq", 1).await;
    assert_eq!(
        dlq_snapshot["messages_ready"], 1,
        "la API de management debe reflejar el mensaje muerto en la .dlq: {dlq_snapshot}"
    );

    // La cola de trabajo debe quedar en 0: el mensaje ya no está pendiente
    // de reintentarse ahí, solo en la .dlq.
    wait_for_messages_ready(&client, &base, "ms-nmap.scan-requests", 0).await;
}
