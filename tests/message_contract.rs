//! Smoke test end-to-end del contrato de mensajes (`docs/verification.md`,
//! Nivel 4): publica un `ScanRequest` de ejemplo válido en `scan.requests`
//! y verifica que llega íntegro a `ms-nmap.scan-requests`; publica las tres
//! variantes de `ScanOutcome` (`started`/`completed`/`failed`) en
//! `scan.outcomes` y verifica el enrutamiento diferenciado ya garantizado
//! por la topología de la feature `topology_definition`
//! (`rabbitmq/definitions.json`): las tres llegan a `gateway.scan-outcomes`,
//! pero SOLO `completed`/`failed` llegan a `ms-analisis.scan-outcomes`.
//!
//! Cada payload usado aquí además valida contra su JSON Schema de
//! `contracts/` (vía `broker_verification::contracts`), para que este test
//! confirme el contrato completo (forma + enrutamiento), no solo uno de los
//! dos.

mod common;

use std::time::Duration;

use broker_verification::contracts::{validate_scan_outcome, validate_scan_request};
use broker_verification::lab_credentials as creds;
use lapin::options::{BasicGetOptions, BasicPublishOptions};
use lapin::BasicProperties;
use serde_json::json;

/// `ssh_credentials_ref` de laboratorio, nunca una credencial real — ver
/// `docs/security-scope.md`.
const LAB_SSH_CREDENTIALS_REF: &str = "lab-only-not-a-real-secret";

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
        .expect("publicar debe aceptarse")
        .await
        .expect("el broker debe confirmar la entrega");
}

/// Espera hasta 2s a que aparezca un mensaje en `queue` (a diferencia de un
/// `basic.get` único: el enrutamiento/entrega puede tardar unos ms).
async fn wait_for_message(channel: &lapin::Channel, queue: &str) -> Option<Vec<u8>> {
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
async fn scan_request_published_arrives_intact_at_ms_nmap_queue() {
    let payload = json!({
        "correlation_id": "corr-e2e-1",
        "ip": "198.51.100.4",
        "network_user": "scanner",
        "ssh_credentials_ref": LAB_SSH_CREDENTIALS_REF,
        "has_sudo": false,
        "requested_by": "analyst@example.test"
    });
    validate_scan_request(&payload).expect("el payload de ejemplo debe cumplir el schema");
    let bytes = serde_json::to_vec(&payload).expect("el payload debe serializar a JSON");

    let container = common::start_broker().await;
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let channel = admin.create_channel().await.expect("canal de admin");

    publish(&channel, "scan.requests", "scan.request", &bytes).await;

    let received = wait_for_message(&channel, "ms-nmap.scan-requests")
        .await
        .expect("el ScanRequest debe llegar a ms-nmap.scan-requests");
    let received_value: serde_json::Value =
        serde_json::from_slice(&received).expect("el mensaje recibido debe ser JSON válido");

    assert_eq!(
        received_value, payload,
        "el mensaje debe llegar íntegro (sin transformaciones) a la cola de ms-nmap"
    );
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn scan_outcome_variants_route_exactly_as_the_topology_declares() {
    let started = json!({"status": "started", "correlation_id": "corr-e2e-started"});
    let completed = json!({
        "status": "completed",
        "correlation_id": "corr-e2e-completed",
        "result": {
            "host": "192.0.2.10",
            "ports": [
                {
                    "port": 22,
                    "protocol": "tcp",
                    "state": "open",
                    "service": "ssh",
                    "version": "OpenSSH 9.6p1",
                    "cpes": ["cpe:/a:openbsd:openssh:9.6p1"]
                }
            ],
            "vulnerabilities": [
                {
                    "id": "CVE-2023-38408",
                    "severity": "high",
                    "description": "ssh-agent PKCS#11 arbitrary code execution",
                    "nse_script": "ssh-vuln-cve2023-38408",
                    "source": "nmap_nse",
                    "references": ["https://www.openssh.com/txt/release-9.3p2"]
                }
            ],
            "scanned_at": "2026-08-27T12:30:00+00:00"
        }
    });
    let failed = json!({
        "status": "failed",
        "correlation_id": "corr-e2e-failed",
        "reason": "etapa SSH: tiempo de espera agotado"
    });

    for payload in [&started, &completed, &failed] {
        validate_scan_outcome(payload).expect("cada variante de ejemplo debe cumplir el schema");
    }

    let container = common::start_broker().await;
    let admin = common::connect_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD).await;
    let channel = admin.create_channel().await.expect("canal de admin");

    publish(
        &channel,
        "scan.outcomes",
        "scan.outcome.started",
        &serde_json::to_vec(&started).unwrap(),
    )
    .await;
    publish(
        &channel,
        "scan.outcomes",
        "scan.outcome.completed",
        &serde_json::to_vec(&completed).unwrap(),
    )
    .await;
    publish(
        &channel,
        "scan.outcomes",
        "scan.outcome.failed",
        &serde_json::to_vec(&failed).unwrap(),
    )
    .await;

    // gateway.scan-outcomes recibe las tres variantes (RF-07/RF-08).
    let mut in_gateway = Vec::new();
    for _ in 0..3 {
        let message = wait_for_message(&channel, "gateway.scan-outcomes")
            .await
            .expect("gateway.scan-outcomes debe recibir las tres variantes");
        in_gateway.push(
            serde_json::from_slice::<serde_json::Value>(&message).expect("mensaje JSON válido"),
        );
    }
    assert!(
        in_gateway.contains(&started)
            && in_gateway.contains(&completed)
            && in_gateway.contains(&failed),
        "gateway.scan-outcomes debe recibir started, completed y failed íntegros: {in_gateway:?}"
    );
    assert_eq!(
        wait_for_message(&channel, "gateway.scan-outcomes").await,
        None,
        "gateway.scan-outcomes no debe recibir mensajes de más"
    );

    // ms-analisis.scan-outcomes recibe SOLO completed y failed, nunca started.
    let mut in_ms_analisis = Vec::new();
    for _ in 0..2 {
        let message = wait_for_message(&channel, "ms-analisis.scan-outcomes")
            .await
            .expect("ms-analisis.scan-outcomes debe recibir completed y failed");
        in_ms_analisis.push(
            serde_json::from_slice::<serde_json::Value>(&message).expect("mensaje JSON válido"),
        );
    }
    assert!(
        in_ms_analisis.contains(&completed) && in_ms_analisis.contains(&failed),
        "ms-analisis.scan-outcomes debe recibir completed y failed íntegros: {in_ms_analisis:?}"
    );
    assert!(
        !in_ms_analisis.contains(&started),
        "ms-analisis.scan-outcomes NUNCA debe recibir la variante started"
    );
    assert_eq!(
        wait_for_message(&channel, "ms-analisis.scan-outcomes").await,
        None,
        "ms-analisis.scan-outcomes no debe recibir mensajes de más (en particular, no started)"
    );
}
