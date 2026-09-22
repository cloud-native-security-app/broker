//! Verifica el mecanismo de reintentos acotados (RNF-06): `x-delivery-limit:
//! 3` en cada cola principal (colas quorum). Un mensaje rechazado con
//! `basic.reject(requeue=true)` se sigue reentregando en la cola original
//! (entrega original + hasta 3 redeliveries) antes de que RabbitMQ lo
//! dead-lettere automáticamente hacia la `.dlq`, sin intervención de ningún
//! consumidor — ver `progress/explore_retry_pattern.md` §3, §5 y §7 para
//! la justificación de N=3, y la nota empírica abajo para detalles del
//! comportamiento observado contra un broker real.
//!
//! **Nota empírica (desviación de `progress/explore_retry_pattern.md`,
//! confirmada contra RabbitMQ 4.3.5 real, no solo por documentación):**
//! el contador que RabbitMQ compara contra `x-delivery-limit` es
//! `x-delivery-count`, que **solo se incrementa con `basic.reject`** (el
//! método AMQP 0-9-1 estándar de un solo mensaje). `basic.nack` con
//! `requeue=true` —el método que sugería la investigación previa— en este
//! contenedor solo incrementó `x-acquired-count` (un contador informativo)
//! y **nunca** disparó el dead-lettering, ni siquiera tras 8 redeliveries.
//! Se comprobó con `basic.reject` (y de forma cruzada con la API HTTP de
//! management, `ackmode: "reject_requeue_true"`, que internamente también
//! usa `basic.reject`) que el límite sí se respeta. Se deja esta nota para
//! que un consumidor real (`ms-nmap`, etc.) sepa que debe usar
//! `basic.reject`, no `basic.nack`, si depende de este mecanismo.
//! Además, justo tras el arranque del contenedor la cola quorum puede
//! tardar un instante en servir `basic.get`/aplicar el rechazo (elección
//! de líder Raft reciente) — este test sondea con reintentos cortos en vez
//! de asumir una única llamada inmediata.

mod common;

use std::time::Duration;

use broker_verification::lab_credentials as creds;
use lapin::message::Delivery;
use lapin::options::{BasicGetOptions, BasicPublishOptions, BasicRejectOptions};
use lapin::types::{AMQPValue, ShortString};
use lapin::BasicProperties;

const LAB_PAYLOAD: &[u8] = br#"{"correlation_id":"lab-only-delivery-limit-test"}"#;
const MAX_REDELIVERIES: u32 = 10;
const POLL_TIMEOUT: Duration = Duration::from_secs(3);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Sondea `basic.get` sobre `queue` hasta obtener un mensaje o agotar
/// `POLL_TIMEOUT`. Necesario porque, justo tras el arranque del
/// contenedor o justo tras un `basic.reject`, una cola quorum puede tardar
/// un instante en reflejar el nuevo estado (consenso Raft de un solo nodo,
/// pero no instantáneo).
async fn poll_get(channel: &lapin::Channel, queue: &str) -> Option<Delivery> {
    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
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
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Busca, dentro del header `x-death` (array de tablas AMQP), la entrada
/// cuyo campo `queue` coincide con `expected_queue`, y devuelve su
/// `reason`. Formato de `x-death` documentado en
/// `progress/explore_retry_pattern.md` §2 / `explore_definitions_format.md` §8.2.
fn x_death_reason_for_queue(delivery: &Delivery, expected_queue: &str) -> String {
    let headers = delivery
        .properties
        .headers()
        .as_ref()
        .expect("el mensaje dead-letrado debe traer headers con x-death");
    let x_death = headers
        .inner()
        .get(&ShortString::from("x-death"))
        .expect("debe existir el header x-death");
    let AMQPValue::FieldArray(entries) = x_death else {
        panic!("x-death debe ser un array de tablas, llegó: {x_death:?}");
    };
    for entry in entries.as_slice() {
        let AMQPValue::FieldTable(table) = entry else {
            continue;
        };
        let queue = table.inner().get(&ShortString::from("queue"));
        if let Some(AMQPValue::LongString(queue)) = queue {
            if queue.as_bytes() == expected_queue.as_bytes() {
                let reason = table
                    .inner()
                    .get(&ShortString::from("reason"))
                    .expect("la entrada de x-death debe traer reason");
                let AMQPValue::LongString(reason) = reason else {
                    panic!("reason debe ser un string, llegó: {reason:?}");
                };
                return String::from_utf8_lossy(reason.as_bytes()).into_owned();
            }
        }
    }
    panic!("no se encontró en x-death una entrada para la cola {expected_queue}: {entries:?}");
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn message_exhausting_delivery_limit_lands_in_dlq_with_delivery_limit_reason() {
    let container = common::start_broker().await;
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let channel = admin.create_channel().await.expect("canal de admin");

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

    // Recibimos y rechazamos (basic.reject, requeue=true) el mismo mensaje
    // repetidamente, simulando fallos de procesamiento transitorios, hasta
    // que dejе de reentregarse en la cola de trabajo (x-delivery-limit
    // agotado) — con un tope de seguridad para no colgar el test si algo
    // va mal.
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

    // No debe haber caído directo a la .dlq en la primera entrega: eso
    // demostraría que NO pasó por el ciclo de reintentos, violando el
    // criterio de aceptación de la feature (RNF-06).
    assert!(
        redeliveries > 1,
        "el mensaje debe haberse reentregado más de una vez antes de agotar x-delivery-limit=3, \
         se contaron {redeliveries} entrega(s)"
    );

    let dead_letter = poll_get(&channel, "ms-nmap.scan-requests.dlq")
        .await
        .expect(
            "tras agotar x-delivery-limit=3 el mensaje debe aparecer en ms-nmap.scan-requests.dlq",
        );

    assert_eq!(dead_letter.data, LAB_PAYLOAD);

    let reason = x_death_reason_for_queue(&dead_letter, "ms-nmap.scan-requests");
    assert_eq!(
        reason, "delivery_limit",
        "x-death debe registrar reason=delivery_limit, prueba de que pasó por el ciclo de \
         reintentos ({redeliveries} entregas) y no cayó directo a la .dlq en el primer fallo"
    );
}
