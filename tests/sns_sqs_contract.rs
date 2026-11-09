//! MSG-04 and MSG-05: the JSON Schema contracts validate the bodies that
//! arrive on SQS after a real SNS publish, and the `routing_key` attribute
//! and subscription filters route exactly as the signed-off topology says.
//! Validation uses `is_ok()` so a failure never prints the offending body
//! (`ssh_credentials_ref`). Runs against LocalStack (`docs/verification.md`, level 3).

mod support;

use std::time::Duration;

use broker_verification::contracts::{
    validate_scan_cancellation, validate_scan_outcome, validate_scan_request,
};
use serde_json::{json, Value};

const WAIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_secs(2);

/// Lab placeholder, never a real credential (`docs/security-scope.md`).
const LAB_SSH_CREDENTIALS_REF: &str = "lab-only-not-a-real-secret";

fn body_of(message: &aws_sdk_sqs::types::Message) -> Value {
    serde_json::from_str(message.body.as_deref().expect("body"))
        .expect("raw delivery: the SQS body is the bare JSON contract")
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn scan_request_arrives_bare_with_routing_key_only_on_the_requests_queue() {
    let payload = json!({
        "correlation_id": "corr-e2e-1",
        "ip": "198.51.100.4",
        "network_user": "scanner",
        "ssh_credentials_ref": LAB_SSH_CREDENTIALS_REF,
        "has_sudo": false,
        "requested_by": "analyst@example.test"
    });
    assert!(
        validate_scan_request(&payload).is_ok(),
        "example must satisfy the schema"
    );
    let env = support::start(None).await;

    env.publish("scan-requests", "scan.request", &payload.to_string())
        .await;

    let received = env.drain("ms-nmap-scan-requests", WAIT).await;
    assert_eq!(received.len(), 1, "exactly one request expected");
    // assert! not assert_eq!: a failure must not print the body (ssh_credentials_ref).
    assert!(
        body_of(&received[0]) == payload,
        "body must arrive untouched"
    );
    assert!(
        validate_scan_request(&body_of(&received[0])).is_ok(),
        "received body must satisfy the schema"
    );
    assert_eq!(
        support::routing_key(&received[0]).as_deref(),
        Some("scan.request")
    );
    for other in ["ms-nmap-scan-cancellations", "gateway-scan-outcomes"] {
        assert!(
            env.drain(other, QUIET).await.is_empty(),
            "{other} must not receive requests"
        );
    }
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn scan_cancellation_arrives_bare_only_on_the_cancellations_queue() {
    let payload = json!({
        "correlation_id": "req-2026-0042",
        "requested_by": "analyst@example.test"
    });
    assert!(
        validate_scan_cancellation(&payload).is_ok(),
        "example must satisfy the schema"
    );
    let env = support::start(None).await;

    env.publish(
        "scan-cancellations",
        "scan.cancellation",
        &payload.to_string(),
    )
    .await;

    let received = env.drain("ms-nmap-scan-cancellations", WAIT).await;
    assert_eq!(received.len(), 1);
    assert!(
        body_of(&received[0]) == payload,
        "body must arrive untouched"
    );
    assert_eq!(
        support::routing_key(&received[0]).as_deref(),
        Some("scan.cancellation")
    );
    for other in ["ms-nmap-scan-requests", "gateway-scan-outcomes"] {
        assert!(
            env.drain(other, QUIET).await.is_empty(),
            "{other} must not receive cancellations"
        );
    }
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn gateway_receives_all_three_outcomes_and_the_filter_drops_other_routing_keys() {
    let started = json!({"status": "started", "correlation_id": "corr-e2e-started"});
    let completed = json!({
        "status": "completed",
        "correlation_id": "corr-e2e-completed",
        "result": {
            "host": "192.0.2.10",
            "ports": [{
                "port": 22,
                "protocol": "tcp",
                "state": "open",
                "service": "ssh",
                "version": "OpenSSH 9.6p1",
                "cpes": ["cpe:/a:openbsd:openssh:9.6p1"]
            }],
            "vulnerabilities": [{
                "id": "CVE-2023-38408",
                "severity": "high",
                "description": "ssh-agent PKCS#11 arbitrary code execution",
                "nse_script": "ssh-vuln-cve2023-38408",
                "source": "nmap_nse",
                "references": ["https://www.openssh.com/txt/release-9.3p2"]
            }],
            "scanned_at": "2026-08-27T12:30:00+00:00"
        }
    });
    let failed = json!({
        "status": "failed",
        "correlation_id": "corr-e2e-failed",
        "reason": "ssh stage: timeout"
    });
    let outcomes = [
        ("scan.outcome.started", &started),
        ("scan.outcome.completed", &completed),
        ("scan.outcome.failed", &failed),
    ];
    for (_, payload) in outcomes {
        validate_scan_outcome(payload).expect("example must satisfy the schema");
    }
    let env = support::start(None).await;

    for (routing_key, payload) in outcomes {
        env.publish("scan-outcomes", routing_key, &payload.to_string())
            .await;
    }
    // Not an outcome: the subscription filter must drop it.
    env.publish("scan-outcomes", "scan.request", r#"{"stray":true}"#)
        .await;

    let mut received = Vec::new();
    while received.len() < 3 {
        let batch = env.drain("gateway-scan-outcomes", WAIT).await;
        assert!(!batch.is_empty(), "gateway must receive all three outcomes");
        received.extend(batch);
    }
    assert_eq!(
        received.len(),
        3,
        "the filter must drop routing keys outside scan.outcome."
    );
    for (routing_key, payload) in outcomes {
        let message = received
            .iter()
            .find(|m| support::routing_key(m).as_deref() == Some(routing_key))
            .unwrap_or_else(|| panic!("missing {routing_key}"));
        assert!(
            &body_of(message) == payload,
            "{routing_key} body must arrive untouched"
        );
        assert!(
            validate_scan_outcome(&body_of(message)).is_ok(),
            "received body must satisfy the schema"
        );
    }
    assert!(
        env.drain("gateway-scan-outcomes", QUIET).await.is_empty(),
        "the filter must drop routing keys outside scan.outcome."
    );
    for other in ["ms-nmap-scan-requests", "ms-nmap-scan-cancellations"] {
        assert!(env.drain(other, QUIET).await.is_empty(), "{other}");
    }
}
