//! Validación del contrato de mensajes (`contracts/`) contra los JSON
//! Schema congelados en la feature `message_contract`.
//!
//! Los schemas se embeben en el binario vía `include_str!` (fuente de
//! verdad: los archivos de `contracts/`, nunca una copia duplicada) y se
//! compilan una sola vez por proceso con el crate `jsonschema` (draft
//! 2020-12, auto-detectado desde `$schema`). Sin lógica de negocio: solo
//! ayuda a los tests de este crate (unitarios e de integración) a afirmar
//! que un payload de ejemplo cumple/incumple el contrato — ver
//! `docs/conventions.md` ("`src/` solo contiene helpers puros").

use std::sync::OnceLock;

use jsonschema::Validator;
use serde_json::Value;

/// JSON Schema de `ScanRequest` (ver `contracts/scan-request.schema.json`
/// y `contracts/README.md`).
pub const SCAN_REQUEST_SCHEMA: &str = include_str!("../contracts/scan-request.schema.json");

/// JSON Schema de `ScanOutcome` (`started`/`completed`/`failed`, ver
/// `contracts/scan-outcome.schema.json` y `contracts/README.md`).
pub const SCAN_OUTCOME_SCHEMA: &str = include_str!("../contracts/scan-outcome.schema.json");

/// JSON Schema de `ScanCancellation` (RF-14, ver
/// `contracts/scan-cancellation.schema.json` y `contracts/README.md`).
pub const SCAN_CANCELLATION_SCHEMA: &str =
    include_str!("../contracts/scan-cancellation.schema.json");

/// JSON Schema de `SecurityEvent` (registros del stream Kinesis
/// `security-events`, ver `contracts/security-event.schema.json` y
/// `contracts/README.md`).
pub const SECURITY_EVENT_SCHEMA: &str = include_str!("../contracts/security-event.schema.json");

/// Errores propios de la validación de un payload contra su JSON Schema.
#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("el schema embebido {schema} no es JSON Schema válido: {detail}")]
    InvalidSchema { schema: String, detail: String },
    #[error("el payload no cumple el schema {schema}: {detail}")]
    SchemaViolation { schema: String, detail: String },
}

fn compile(schema_name: &str, schema_source: &str) -> Validator {
    let schema: Value = serde_json::from_str(schema_source).unwrap_or_else(|err| {
        panic!("{schema_name} debe ser JSON válido (embebido en el binario): {err}")
    });
    jsonschema::validator_for(&schema).unwrap_or_else(|err| {
        panic!("{schema_name} debe ser JSON Schema draft 2020-12 válido: {err}")
    })
}

fn scan_request_validator() -> &'static Validator {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| compile("scan-request.schema.json", SCAN_REQUEST_SCHEMA))
}

fn scan_outcome_validator() -> &'static Validator {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| compile("scan-outcome.schema.json", SCAN_OUTCOME_SCHEMA))
}

fn scan_cancellation_validator() -> &'static Validator {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| compile("scan-cancellation.schema.json", SCAN_CANCELLATION_SCHEMA))
}

fn security_event_validator() -> &'static Validator {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| compile("security-event.schema.json", SECURITY_EVENT_SCHEMA))
}

/// Valida `payload` contra `contracts/scan-request.schema.json`.
///
/// # Errors
///
/// Devuelve [`ContractError::SchemaViolation`] con el detalle del primer
/// error de validación si `payload` no cumple el schema.
pub fn validate_scan_request(payload: &Value) -> Result<(), ContractError> {
    scan_request_validator()
        .validate(payload)
        .map_err(|err| ContractError::SchemaViolation {
            schema: "scan-request.schema.json".to_string(),
            detail: err.to_string(),
        })
}

/// Valida `payload` contra `contracts/scan-outcome.schema.json` (cualquiera
/// de las tres variantes `started`/`completed`/`failed`).
///
/// # Errors
///
/// Devuelve [`ContractError::SchemaViolation`] con el detalle del primer
/// error de validación si `payload` no cumple ninguna variante del schema.
pub fn validate_scan_outcome(payload: &Value) -> Result<(), ContractError> {
    scan_outcome_validator()
        .validate(payload)
        .map_err(|err| ContractError::SchemaViolation {
            schema: "scan-outcome.schema.json".to_string(),
            detail: err.to_string(),
        })
}

/// Valida `payload` contra `contracts/scan-cancellation.schema.json`.
///
/// # Errors
///
/// Devuelve [`ContractError::SchemaViolation`] con el detalle del primer
/// error de validación si `payload` no cumple el schema.
pub fn validate_scan_cancellation(payload: &Value) -> Result<(), ContractError> {
    scan_cancellation_validator()
        .validate(payload)
        .map_err(|err| ContractError::SchemaViolation {
            schema: "scan-cancellation.schema.json".to_string(),
            detail: err.to_string(),
        })
}

/// Valida `event` contra `contracts/security-event.schema.json` (sobre más
/// el payload propio de su `event_type`).
///
/// # Errors
///
/// Devuelve [`ContractError::SchemaViolation`] con el detalle del primer
/// error de validación si `event` no cumple el schema.
pub fn validate_security_event(event: &Value) -> Result<(), ContractError> {
    security_event_validator()
        .validate(event)
        .map_err(|err| ContractError::SchemaViolation {
            schema: "security-event.schema.json".to_string(),
            detail: err.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `ssh_credentials_ref` de laboratorio, nunca una credencial real —
    /// ver `docs/security-scope.md`.
    const LAB_SSH_CREDENTIALS_REF: &str = "lab-only-not-a-real-secret";

    fn valid_scan_request() -> Value {
        json!({
            "correlation_id": "req-2026-0042",
            "ip": "198.51.100.4",
            "network_user": "scanner",
            "ssh_credentials_ref": LAB_SSH_CREDENTIALS_REF,
            "has_sudo": false,
            "requested_by": "analyst@example.test"
        })
    }

    /// Payload real de ejemplo citado en
    /// `docs/decisions.md` (D-08) (test
    /// `completed_outcome_serializes_to_expected_message_shape` de
    /// `ms-nmap`).
    fn valid_scan_outcome_completed() -> Value {
        json!({
            "status": "completed",
            "correlation_id": "corr-42",
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
        })
    }

    /// Payload real de ejemplo citado en
    /// `docs/decisions.md` (D-08) (test
    /// `failed_outcome_serializes_with_correlation_id_and_reason` de
    /// `ms-nmap`).
    fn valid_scan_outcome_failed() -> Value {
        json!({
            "status": "failed",
            "correlation_id": "corr-42",
            "reason": "etapa SSH: tiempo de espera agotado"
        })
    }

    /// Shape acordado para RF-07/RF-08 (contrato acordado, pendiente en
    /// `ms-nmap` — ver `contracts/README.md`).
    fn valid_scan_outcome_started() -> Value {
        json!({
            "status": "started",
            "correlation_id": "corr-1"
        })
    }

    #[test]
    fn valid_scan_request_with_lab_credential_passes() {
        let payload = valid_scan_request();
        assert!(
            validate_scan_request(&payload).is_ok(),
            "un ScanRequest válido con ssh_credentials_ref de laboratorio debe pasar el schema"
        );
    }

    #[test]
    fn scan_request_missing_correlation_id_is_rejected() {
        let mut payload = valid_scan_request();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .remove("correlation_id");

        let result = validate_scan_request(&payload);
        let err = result.expect_err("un ScanRequest sin correlation_id debe ser inválido");
        let message = err.to_string();
        assert!(
            message.contains("scan-request.schema.json"),
            "el mensaje de error debe identificar el schema violado: {message}"
        );
    }

    #[test]
    fn scan_request_with_unknown_extra_field_is_rejected() {
        let mut payload = valid_scan_request();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .insert("unexpected_field".to_string(), json!("nope"));

        assert!(
            validate_scan_request(&payload).is_err(),
            "additionalProperties: false debe rechazar campos no documentados"
        );
    }

    #[test]
    fn scan_request_with_invalid_ip_is_rejected() {
        let mut payload = valid_scan_request();
        payload["ip"] = json!("not-an-ip-address");

        assert!(
            validate_scan_request(&payload).is_err(),
            "un ip que no es IPv4 ni IPv6 debe ser inválido"
        );
    }

    #[test]
    fn valid_scan_outcome_started_passes() {
        assert!(
            validate_scan_outcome(&valid_scan_outcome_started()).is_ok(),
            "la variante started (contrato acordado) debe validar contra el schema"
        );
    }

    #[test]
    fn valid_scan_outcome_completed_passes() {
        assert!(
            validate_scan_outcome(&valid_scan_outcome_completed()).is_ok(),
            "el payload real completed de ms-nmap debe validar contra el schema"
        );
    }

    #[test]
    fn valid_scan_outcome_failed_passes() {
        assert!(
            validate_scan_outcome(&valid_scan_outcome_failed()).is_ok(),
            "el payload real failed de ms-nmap debe validar contra el schema"
        );
    }

    #[test]
    fn scan_outcome_completed_with_reason_is_rejected() {
        let mut payload = valid_scan_outcome_completed();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .insert("reason".to_string(), json!("no debería estar aquí"));

        assert!(
            validate_scan_outcome(&payload).is_err(),
            "completed con un reason colado debe ser inválido (additionalProperties: false)"
        );
    }

    #[test]
    fn scan_outcome_failed_with_result_is_rejected() {
        let mut payload = valid_scan_outcome_failed();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .insert(
                "result".to_string(),
                json!({
                    "host": "192.0.2.10",
                    "ports": [],
                    "vulnerabilities": [],
                    "scanned_at": "2026-08-27T12:30:00+00:00"
                }),
            );

        assert!(
            validate_scan_outcome(&payload).is_err(),
            "failed con un result colado debe ser inválido (additionalProperties: false)"
        );
    }

    #[test]
    fn scan_outcome_missing_correlation_id_is_rejected() {
        let mut payload = valid_scan_outcome_failed();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .remove("correlation_id");

        let result = validate_scan_outcome(&payload);
        let err = result.expect_err("un ScanOutcome sin correlation_id debe ser inválido");
        let message = err.to_string();
        assert!(
            message.contains("scan-outcome.schema.json"),
            "el mensaje de error debe identificar el schema violado: {message}"
        );
    }

    #[test]
    fn scan_outcome_with_unknown_status_is_rejected() {
        let payload = json!({
            "status": "queued",
            "correlation_id": "corr-1"
        });

        assert!(
            validate_scan_outcome(&payload).is_err(),
            "un status fuera de started/completed/failed debe ser inválido"
        );
    }

    /// Shape acordado para RF-14 (`cancellation_contract`) — ver
    /// `contracts/README.md` §`ScanCancellation`.
    fn valid_scan_cancellation() -> Value {
        json!({
            "correlation_id": "req-2026-0042",
            "requested_by": "analyst@example.test"
        })
    }

    #[test]
    fn valid_scan_cancellation_passes() {
        assert!(
            validate_scan_cancellation(&valid_scan_cancellation()).is_ok(),
            "un ScanCancellation válido debe pasar el schema"
        );
    }

    #[test]
    fn scan_cancellation_missing_correlation_id_is_rejected() {
        let mut payload = valid_scan_cancellation();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .remove("correlation_id");

        let result = validate_scan_cancellation(&payload);
        let err = result.expect_err("un ScanCancellation sin correlation_id debe ser inválido");
        let message = err.to_string();
        assert!(
            message.contains("scan-cancellation.schema.json"),
            "el mensaje de error debe identificar el schema violado: {message}"
        );
    }

    #[test]
    fn scan_cancellation_missing_requested_by_is_rejected() {
        let mut payload = valid_scan_cancellation();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .remove("requested_by");

        assert!(
            validate_scan_cancellation(&payload).is_err(),
            "un ScanCancellation sin requested_by debe ser inválido"
        );
    }

    #[test]
    fn scan_cancellation_with_unknown_extra_field_is_rejected() {
        let mut payload = valid_scan_cancellation();
        payload
            .as_object_mut()
            .expect("payload es un objeto")
            .insert(
                "ssh_credentials_ref".to_string(),
                json!("should-never-be-here"),
            );

        assert!(
            validate_scan_cancellation(&payload).is_err(),
            "additionalProperties: false debe rechazar campos no documentados, en particular \
             cualquier intento de colar una credencial en este mensaje"
        );
    }

    // ---- SecurityEvent (Kinesis `security-events`) ----

    const SECURITY_EVENT_EXAMPLES_DIR: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/contracts/examples/security-event"
    );

    const SECURITY_EVENT_TYPES: [&str; 11] = [
        "scan.requested",
        "scan.cancelled",
        "scan.started",
        "scan.completed",
        "scan.failed",
        "finding.critical",
        "auth.login",
        "auth.logout",
        "ratelimit.hit",
        "message.deadlettered",
        "audit.recorded",
    ];

    fn envelope(event_type: &str, source: &str, payload: Value) -> Value {
        json!({
            "event_id": "3f2b8c1e-5d4a-4e7b-9a10-2c6d8e0f1a42",
            "event_type": event_type,
            "occurred_at": "2026-11-09T12:30:00Z",
            "source": source,
            "actor_sub": "user-sub-123",
            "scan_id": "corr-42",
            "payload": payload
        })
    }

    fn valid_security_events() -> Vec<Value> {
        let counts =
            json!({"critical": 1, "high": 2, "medium": 3, "low": 4, "info": 5, "unknown": 0});
        vec![
            envelope(
                "scan.requested",
                "gateway",
                json!({"target": "198.51.100.4"}),
            ),
            envelope("scan.cancelled", "gateway", json!({})),
            envelope("scan.started", "nmap-service", json!({})),
            envelope(
                "scan.completed",
                "nmap-service",
                json!({"duration_ms": 42000, "finding_counts": counts}),
            ),
            envelope(
                "scan.failed",
                "nmap-service",
                json!({"reason_code": "ssh_timeout"}),
            ),
            envelope(
                "finding.critical",
                "nmap-service",
                json!({
                    "port": 22, "protocol": "tcp", "cve_id": "CVE-2023-38408",
                    "severity": "critical", "confidence": "confirmed"
                }),
            ),
            envelope("auth.login", "gateway", json!({})),
            envelope("auth.logout", "gateway", json!({})),
            envelope(
                "ratelimit.hit",
                "gateway",
                json!({"route": "/api/scans", "limit": 10, "window_seconds": 60}),
            ),
            envelope(
                "message.deadlettered",
                "gateway",
                json!({"queue": "gateway-scan-outcomes", "reason_code": "malformed_message"}),
            ),
            envelope(
                "audit.recorded",
                "user-service",
                json!({"action": "user.updated", "resource_type": "user"}),
            ),
        ]
    }

    fn security_event(event_type: &str) -> Value {
        valid_security_events()
            .into_iter()
            .find(|e| e["event_type"] == event_type)
            .expect("fixture for event type")
    }

    #[test]
    fn every_security_event_type_has_a_valid_fixture() {
        let events = valid_security_events();
        for event_type in SECURITY_EVENT_TYPES {
            let event = events
                .iter()
                .find(|e| e["event_type"] == *event_type)
                .unwrap_or_else(|| panic!("missing fixture for {event_type}"));
            validate_security_event(event)
                .unwrap_or_else(|err| panic!("{event_type} must validate: {err}"));
        }
    }

    #[test]
    fn security_event_without_optional_ids_passes_when_not_required() {
        // scan.* needs scan_id but not actor_sub; auth.* needs actor_sub but not scan_id.
        let mut scan = security_event("scan.started");
        scan.as_object_mut().unwrap().remove("actor_sub");
        assert!(validate_security_event(&scan).is_ok());

        let mut auth = security_event("auth.login");
        auth.as_object_mut().unwrap().remove("scan_id");
        assert!(validate_security_event(&auth).is_ok());
    }

    #[test]
    fn security_event_with_unknown_type_is_rejected() {
        let event = envelope("scan.exploded", "gateway", json!({}));
        assert!(validate_security_event(&event).is_err());
    }

    #[test]
    fn security_event_missing_envelope_fields_is_rejected() {
        for field in ["event_id", "event_type", "occurred_at", "source", "payload"] {
            let mut event = security_event("auth.login");
            event.as_object_mut().unwrap().remove(field);
            assert!(
                validate_security_event(&event).is_err(),
                "removing {field} must be rejected"
            );
        }
    }

    #[test]
    fn security_event_with_malformed_envelope_values_is_rejected() {
        let cases = [
            ("event_id", json!("not-a-uuid")),
            ("occurred_at", json!("yesterday")),
            ("source", json!("browser")),
            ("actor_sub", json!("")),
            ("scan_id", json!("")),
        ];
        for (field, value) in cases {
            let mut event = security_event("scan.started");
            event[field] = value;
            assert!(
                validate_security_event(&event).is_err(),
                "bad {field} must be rejected"
            );
        }
    }

    #[test]
    fn security_event_with_wrong_payload_for_type_is_rejected() {
        let mut event = security_event("ratelimit.hit");
        event["payload"] = json!({"target": "198.51.100.4"});
        assert!(validate_security_event(&event).is_err());

        let mut event = security_event("scan.completed");
        event["payload"] = json!({"reason_code": "ssh_timeout"});
        assert!(validate_security_event(&event).is_err());
    }

    #[test]
    fn security_event_with_forbidden_extra_fields_is_rejected() {
        for field in [
            "body",
            "password",
            "token",
            "ssh_credentials_ref",
            "raw_output",
        ] {
            let mut payload_event = security_event("message.deadlettered");
            payload_event["payload"][field] = json!("secret");
            assert!(
                validate_security_event(&payload_event).is_err(),
                "payload field {field} must be rejected"
            );

            let mut envelope_event = security_event("message.deadlettered");
            envelope_event[field] = json!("secret");
            assert!(
                validate_security_event(&envelope_event).is_err(),
                "envelope field {field} must be rejected"
            );
        }
        let mut empty = security_event("auth.login");
        empty["payload"] = json!({"password": "x"});
        assert!(validate_security_event(&empty).is_err());
    }

    #[test]
    fn scan_and_finding_events_require_scan_id() {
        for event_type in [
            "scan.requested",
            "scan.cancelled",
            "scan.started",
            "scan.completed",
            "scan.failed",
            "finding.critical",
        ] {
            let mut event = security_event(event_type);
            event.as_object_mut().unwrap().remove("scan_id");
            assert!(
                validate_security_event(&event).is_err(),
                "{event_type} without scan_id must be rejected"
            );
        }
    }

    #[test]
    fn auth_ratelimit_and_audit_events_require_actor_sub() {
        for event_type in [
            "auth.login",
            "auth.logout",
            "ratelimit.hit",
            "audit.recorded",
        ] {
            let mut event = security_event(event_type);
            event.as_object_mut().unwrap().remove("actor_sub");
            assert!(
                validate_security_event(&event).is_err(),
                "{event_type} without actor_sub must be rejected"
            );
        }
    }

    #[test]
    fn finding_critical_rejects_medium_severity_and_bad_fields() {
        let mut event = security_event("finding.critical");
        event["payload"]["severity"] = json!("medium");
        assert!(validate_security_event(&event).is_err());

        let mut event = security_event("finding.critical");
        event["payload"]["confidence"] = json!("guess");
        assert!(validate_security_event(&event).is_err());

        let mut event = security_event("finding.critical");
        event["payload"]["port"] = json!(70000);
        assert!(validate_security_event(&event).is_err());

        let mut event = security_event("finding.critical");
        event["payload"]["cve_id"] = Value::Null;
        assert!(
            validate_security_event(&event).is_ok(),
            "cve_id is nullable"
        );
    }

    #[test]
    fn scan_failed_reason_must_be_a_code_not_free_text() {
        let mut event = security_event("scan.failed");
        event["payload"]["reason_code"] = json!("etapa SSH: tiempo de espera agotado");
        assert!(validate_security_event(&event).is_err());
    }

    #[test]
    fn all_security_event_examples_validate_and_cover_every_type() {
        let mut seen = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(SECURITY_EVENT_EXAMPLES_DIR).expect("examples dir") {
            let path = entry.expect("dir entry").path();
            let text = std::fs::read_to_string(&path).expect("read example");
            let event: Value = serde_json::from_str(&text).expect("example is JSON");
            validate_security_event(&event)
                .unwrap_or_else(|err| panic!("{} must validate: {err}", path.display()));
            seen.insert(
                event["event_type"]
                    .as_str()
                    .expect("event_type")
                    .to_string(),
            );
        }
        for event_type in SECURITY_EVENT_TYPES {
            assert!(seen.contains(event_type), "no example for {event_type}");
        }
    }
}
