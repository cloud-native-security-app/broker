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
    /// `progress/explore_ms_nmap_contract.md` §2 (test
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
    /// `progress/explore_ms_nmap_contract.md` §2 (test
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
}
