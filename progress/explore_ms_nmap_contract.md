# Investigación: shape real de `ScanRequest`/`ScanOutcome`/`ScanResult` en `ms-nmap`

> Hecha por el leader directamente (lectura del repo hermano
> `/home/o-aguirre/Documents/duoc/cloud-native/security-app/nmap-service`,
> rama `feature/domain_model`), no por un subagente — se documenta aquí para
> que el `implementer` de la feature `message_contract` no tenga que
> re-derivar el shape "desde cero" (prohibido explícitamente por la
> `description` de la feature en `feature_list.json`).

Fuentes leídas literalmente (rutas absolutas, con número de línea donde
aplica):
- `nmap-service/src/domain.rs` (líneas 129–298): `ScanRequest`, `ScanResult`,
  `PortFinding`, `VulnFinding`, y los enums `Protocol`/`PortState`/`Severity`/
  `VulnSource`.
- `nmap-service/src/messaging/publisher.rs` (líneas 62–91, docstring del
  módulo líneas 22–44): `ScanOutcome` (dos variantes: `Completed`/`Failed`,
  **NO existe `Started` todavía** en el código real — confirma lo que ya dice
  `feature_list.json`: la variante `started` es "acordada pero pendiente de
  implementar en ms-nmap").

---

## 1. `ScanRequest` (domain.rs líneas 133–156)

```rust
pub struct ScanRequest {
    pub correlation_id: CorrelationId,      // newtype #[serde(transparent)] sobre String
    pub ip: IpAddr,                          // serde serializa IpAddr como string (v4 o v6)
    pub network_user: String,
    pub ssh_credentials_ref: SshCredentialsRef, // ver nota de seguridad abajo
    pub has_sudo: bool,
    pub requested_by: String,
}
```

JSON real de ejemplo (test `scan_request_deserializes_real_credential_from_broker_message`,
domain.rs líneas 429–437 — este es el JSON que **ms-nmap recibe desde el
Broker**, publicado por quien crea la solicitud):

```json
{
  "correlation_id": "corr-7",
  "ip": "198.51.100.4",
  "network_user": "scanner",
  "ssh_credentials_ref": "real-broker-secret",
  "has_sudo": false,
  "requested_by": "analyst@example.test"
}
```

**Nota de seguridad clave** (confirma `docs/security-scope.md`): `SshCredentialsRef`
serializa SIEMPRE como `"[REDACTED]"` (nunca expone el secreto al serializar
desde `ms-nmap`), pero **deserializa el valor real** — es decir, en el
mensaje que viaja por el Broker (publicado por Gateway/quien sea, consumido
por `ms-nmap`), `ssh_credentials_ref` es un **string plano con la credencial
real**. El JSON Schema de `scan-request.schema.json` debe tipar
`ssh_credentials_ref` como `string` sin más (no hay forma de que el schema
JSON distinga "redactado" de "real" — es responsabilidad de cada servicio no
loggear el cuerpo, ver `docs/security-scope.md` §"Retención y logging").

Confirma exactamente el shape ya escrito en el criterio de aceptación de
`feature_list.json` para la feature 4: 6 campos, todos requeridos, sin
propiedades adicionales.

---

## 2. `ScanOutcome` (publisher.rs líneas 70–91)

```rust
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ScanOutcome {
    Completed { correlation_id: CorrelationId, result: ScanResult },
    Failed { correlation_id: CorrelationId, reason: String },
}
```

*Internally tagged* por `status` (`"completed"` / `"failed"`). **No existe
variante `Started`** en el código real de `ms-nmap` a día de hoy — coincide
con lo que pide `feature_list.json`: la variante `started` para RF-07/RF-08
se documenta en `contracts/README.md` como "contrato acordado, publicación
pendiente de implementar en ms-nmap", **nunca** afirmando que ms-nmap ya la
envía.

JSON real de ejemplo — éxito (test `completed_outcome_serializes_to_expected_message_shape`,
publisher.rs líneas 339–365, más el ejemplo del docstring del módulo líneas
27–32):

```json
{
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
}
```

JSON real de ejemplo — fallo (test `failed_outcome_serializes_with_correlation_id_and_reason`,
publisher.rs líneas 368–381, y docstring líneas 34–38):

```json
{
  "status": "failed",
  "correlation_id": "corr-42",
  "reason": "etapa SSH: tiempo de espera agotado"
}
```

**Importante para el schema**: el caso `completed` NO lleva `reason`; el caso
`failed` NO lleva `result` (confirmado por los asserts
`json.get("reason").is_none()` / `json.get("result").is_none()` en los tests
citados) — el JSON Schema debe usar `"additionalProperties": false` por
variante (vía `oneOf`/`if-then-else` o 3 sub-schemas con `unevaluatedProperties:
false`) para que un mensaje `completed` con un `reason` colado (o viceversa)
sea inválido.

### 2.1 Shape interno de `ScanResult` (domain.rs líneas 213–298, contrastado, NO derivado a mano)

```rust
pub struct ScanResult {
    pub host: IpAddr,                       // string (v4 o v6), igual que ScanRequest.ip
    pub ports: Vec<PortFinding>,
    pub vulnerabilities: Vec<VulnFinding>,
    #[serde(with = "time::serde::rfc3339")]
    pub scanned_at: OffsetDateTime,          // string RFC 3339 (offset UTC incluido)
}

pub struct PortFinding {
    pub port: u16,                           // integer, 0-65535
    pub protocol: Protocol,                  // "tcp" | "udp" (rename_all = "lowercase")
    pub state: PortState,                    // ver enum abajo (rename_all = "snake_case")
    pub service: Option<String>,             // nullable
    pub version: Option<String>,             // nullable
    #[serde(default)]
    pub cpes: Vec<String>,                   // default [] si el campo falta al deserializar
}

pub struct VulnFinding {
    pub id: Option<String>,                  // nullable
    pub severity: Severity,                  // ver enum abajo (rename_all = "lowercase")
    pub description: String,
    pub nse_script: String,                  // "" si no viene de un script NSE
    #[serde(default = "default_vuln_source")] // default: "nmap_nse"
    pub source: VulnSource,                  // ver enum abajo (rename_all = "snake_case")
    #[serde(default)]
    pub references: Vec<String>,             // default []
}
```

Enums (valores exactos tras `serde(rename_all = ...)`, confirmados por el
test `enums_use_a_stable_string_encoding`, domain.rs líneas 448–473):

- `Protocol`: `"tcp"`, `"udp"`.
- `PortState`: `"open"`, `"closed"`, `"filtered"`, `"unfiltered"`,
  `"open_filtered"`, `"closed_filtered"`.
- `Severity`: `"unknown"`, `"info"`, `"low"`, `"medium"`, `"high"`,
  `"critical"`.
- `VulnSource`: `"nmap_nse"`, `"exploit_db"`, `"nvd"`.

**Nota sobre `cpes`/`references`/`source`**: llevan `#[serde(default...)]`
del lado de `ms-nmap` porque son campos añadidos en features posteriores a
la persistencia original (ver comentario de `default_vuln_source`,
domain.rs líneas 275–280) — eso es una tolerancia de **deserialización** de
`ms-nmap` sobre documentos viejos de MongoDB, no dice nada sobre si el
Broker debe aceptar mensajes sin esos campos. Para el JSON Schema del
**mensaje del Broker** (esta feature), lo correcto es marcarlos como
`required` igualmente — el mensaje que `ms-nmap` *publica* siempre los
incluye (aunque sea con array vacío `[]`), y no hay ninguna necesidad de que
el contrato de mensajería sea más laxo que lo que el propio `ms-nmap`
emite. Si el `implementer` prefiere no exigirlos (permitir que falten y
asumir `[]`)/`"nmap_nse"` por defecto, debe dejarlo documentado como
decisión explícita en `contracts/README.md`, no implícito.

---

## 3. Qué NO se puede confirmar desde este repo (documentar como tal, no inventar)

- La variante `started` de `ScanOutcome` (RF-07/RF-08): no existe en el
  código real de `ms-nmap` — el shape `{status: "started", correlation_id}`
  (sin `result` ni `reason`) viene dado por el propio criterio de aceptación
  de `feature_list.json`, no por un ejemplo real de `ms-nmap`. Documentar
  explícitamente en `contracts/README.md` como "contrato acordado,
  publicación PENDIENTE en ms-nmap" (ya lo exige el criterio de aceptación).
- No hay `schema_version` ni campo de versión en ningún tipo real — no se
  inventa uno aquí (ya lo dice `feature_list.json`: "sin política de
  versionado nueva").

## 4. Resumen accionable para el `implementer`

1. `contracts/scan-request.schema.json`: 6 campos top-level, todos
   `required`, `additionalProperties: false`, shapes según §1.
2. `contracts/scan-outcome.schema.json`: `oneOf` de 3 variantes por
   `status` (`started`/`completed`/`failed`), cada una con
   `additionalProperties: false` y solo sus campos propios (§2, §2.1) —
   `started` con solo `status`+`correlation_id` (contrato acordado, no
   implementado aún); `completed` con `result` anidado tipado completo
   según §2.1 (host, ports[], vulnerabilities[], scanned_at); `failed` con
   `reason`.
3. `contracts/README.md`: documentar la variante `started` como pendiente
   en `ms-nmap` (no implementada), el enrutamiento exchange/routing-key
   (ya definido en `rabbitmq/definitions.json` de la feature 2: `scan.requests`
   con `scan.request`; `scan.outcomes` con `scan.outcome.started/completed/failed`,
   con el binding diferenciado entre `ms-analisis.scan-outcomes` —solo
   completed/failed— y `gateway.scan-outcomes` —las tres—), quién
   publica/consume cada uno, y la ausencia de `schema_version`.
4. Crate de validación JSON Schema en Rust para los tests unitarios
   (sin Docker): usar `jsonschema` (crates.io), soporta draft 2020-12,
   es el más usado/mantenido del ecosistema Rust para este propósito. No
   hace falta investigación adicional — es una elección de bajo riesgo.
