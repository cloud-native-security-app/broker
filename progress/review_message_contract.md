# Review — feature 4 (message_contract)

**Veredicto:** APPROVED

## Método de verificación

Se comparó campo por campo `contracts/scan-request.schema.json` y
`contracts/scan-outcome.schema.json` contra el código fuente real de
`ms-nmap` (no solo contra `progress/explore_ms_nmap_contract.md` ni el
informe del implementer):

- `nmap-service/src/domain.rs` líneas 129–298 (`ScanRequest`, `Protocol`,
  `PortState`, `Severity`, `PortFinding`, `VulnSource`, `VulnFinding`,
  `ScanResult`).
- `nmap-service/src/messaging/publisher.rs` líneas 1–100 (docstring del
  módulo y `enum ScanOutcome` con `#[serde(tag = "status", rename_all =
  "snake_case")]`, variantes `Completed`/`Failed` — confirmado que **no
  existe** variante `Started` en el código real).

Se ejecutó `cargo test` (11 unitarios) y `cargo test -- --ignored` (14 de
integración, incluidos los 2 nuevos de `tests/message_contract.rs`) contra
Docker real, y `./init.sh` completo de punta a punta.

## Comparación campo por campo

### `ScanRequest` (domain.rs 133–156) vs `contracts/scan-request.schema.json`

| Campo real | Tipo real | Schema | Coincide |
|---|---|---|---|
| `correlation_id` | `CorrelationId` (`#[serde(transparent)]` sobre `String`) | `string`, `minLength:1`, required | Sí |
| `ip` | `IpAddr` (serializa como string v4/v6) | `string`, `anyOf` regex IPv4/IPv6, required | Sí |
| `network_user` | `String` | `string`, required | Sí |
| `ssh_credentials_ref` | `SshCredentialsRef` (deserializa el secreto real) | `string`, required | Sí |
| `has_sudo` | `bool` | `boolean`, required | Sí |
| `requested_by` | `String` | `string`, required | Sí |

6 campos, los 6 `required`, `additionalProperties: false` — coincide
exactamente con `struct ScanRequest`. La decisión de usar `pattern` en vez
de `format: ipv4/ipv6` está justificada correctamente (confirmé que
`jsonschema` 0.56 no valida `format` por defecto en 2020-12; con `oneOf` +
dos `format` sin activar `should_validate_formats` ambas ramas habrían sido
`{"type":"string"}` y `oneOf` habría rechazado cualquier IP válida por
matchear las dos ramas a la vez — el análisis del implementer es correcto).

### `ScanOutcome` (publisher.rs 70–91) vs `contracts/scan-outcome.schema.json`

- `Completed { correlation_id, result }` → `$defs/completed`: `status`
  const `"completed"`, `correlation_id`, `result: $ref scanResult`, los 3
  `required`, `additionalProperties: false`. Coincide.
- `Failed { correlation_id, reason }` → `$defs/failed`: `status` const
  `"failed"`, `correlation_id`, `reason`, los 3 `required`,
  `additionalProperties: false`. Coincide.
- `started` → `$defs/started`: solo `status`+`correlation_id`,
  `additionalProperties: false`. No existe en el código real de `ms-nmap`
  (confirmé personalmente en `publisher.rs`: el enum solo tiene
  `Completed`/`Failed`) — correctamente documentado en
  `contracts/README.md` líneas 74–86 como "contrato acordado, publicación
  PENDIENTE en `ms-nmap`", nunca como ya implementado. Coincide con el
  criterio de aceptación de `feature_list.json` id 4.
- Test `completed_outcome_serializes_to_expected_message_shape` /
  `failed_outcome_serializes_with_correlation_id_and_reason` confirman
  (vía asserts citados en `progress/explore_ms_nmap_contract.md`) que
  `completed` no lleva `reason` y `failed` no lleva `result` — el schema lo
  aplica con `additionalProperties: false` por variante, y además está
  probado funcionalmente (no solo declarativo) en
  `src/contracts.rs::scan_outcome_completed_with_reason_is_rejected` y
  `::scan_outcome_failed_with_result_is_rejected`, ambos verdes.

### `ScanResult`/`PortFinding`/`VulnFinding` (domain.rs 213–298) vs `$defs/scanResult` etc.

| Campo real | Schema | Coincide |
|---|---|---|
| `ScanResult.host: IpAddr` | `string` anyOf IPv4/IPv6, required | Sí |
| `ScanResult.ports: Vec<PortFinding>` | array de `$defs/portFinding`, required | Sí |
| `ScanResult.vulnerabilities: Vec<VulnFinding>` | array de `$defs/vulnFinding`, required | Sí |
| `ScanResult.scanned_at` (RFC3339 vía `time::serde::rfc3339`) | `string`, `format: date-time`, required | Sí |
| `PortFinding.port: u16` | `integer`, min 0 max 65535, required | Sí |
| `PortFinding.protocol: Protocol` | `enum: [tcp, udp]`, required | Sí — coincide con `#[serde(rename_all="lowercase")]` |
| `PortFinding.state: PortState` | `enum` de 6 valores exactos (`open`, `closed`, `filtered`, `unfiltered`, `open_filtered`, `closed_filtered`), required | Sí — coincide con `#[serde(rename_all="snake_case")]` |
| `PortFinding.service: Option<String>` | `type: ["string","null"]`, required | Sí (nullable pero required, correcto para `Option` sin `#[serde(default)]`) |
| `PortFinding.version: Option<String>` | `type: ["string","null"]`, required | Sí |
| `PortFinding.cpes: Vec<String>` (`#[serde(default)]` del lado de deserialización de `ms-nmap`) | `array` de `string`, required | Coincide con la decisión documentada (el mensaje que `ms-nmap` publica siempre incluye el campo, aunque sea `[]`) |
| `VulnFinding.id: Option<String>` | `["string","null"]`, required | Sí |
| `VulnFinding.severity: Severity` | `enum` de 6 valores exactos (`unknown`,`info`,`low`,`medium`,`high`,`critical`), required | Sí — coincide con `#[serde(rename_all="lowercase")]` |
| `VulnFinding.description: String` | `string`, required | Sí |
| `VulnFinding.nse_script: String` | `string`, required | Sí |
| `VulnFinding.source: VulnSource` (`#[serde(default=default_vuln_source)]`) | `enum: [nmap_nse, exploit_db, nvd]`, required | Sí — coincide con `#[serde(rename_all="snake_case")]`, decisión de marcarlo `required` documentada |
| `VulnFinding.references: Vec<String>` (`#[serde(default)]`) | array de `string`, required | Sí, decisión documentada |

Los 4 enums fueron verificados uno a uno contra las definiciones reales en
`domain.rs` (líneas 159–249) y coinciden exactamente en cantidad y valores
string con lo que describe `progress/explore_ms_nmap_contract.md` §2.1
(que a su vez cita el test `enums_use_a_stable_string_encoding`). No hay
discrepancias.

## `contracts/README.md`

- Marca `started` explícitamente como "PENDIENTE en `ms-nmap`" (líneas
  17, 74–86), nunca afirma que ya se envía. Correcto.
- Documenta exchange/routing-key/publicador/consumidor de cada mensaje
  (líneas 22–95), contrastado contra `rabbitmq/definitions.json` real: las
  colas (`ms-nmap.scan-requests`, `ms-analisis.scan-outcomes`,
  `gateway.scan-outcomes`), bindings diferenciados
  (`scan.outcome.completed`/`failed` → `ms-analisis.scan-outcomes`;
  `scan.outcome.#` → `gateway.scan-outcomes`) y permisos de usuarios
  (`gateway` escribe `scan.requests`, `ms-nmap` escribe `scan.outcomes`)
  coinciden exactamente con `rabbitmq/definitions.json` líneas 39–68 y
  170–227.
- Nota explícita de ausencia de `schema_version` (líneas 112–123), con
  política de "cambios deben ser aditivos" hasta que se acuerde
  versionado — cumple el criterio de aceptación.
- Nota menor no bloqueante: la tabla de la línea 16
  (`| scan-request.schema.json | ScanRequest | Implementado y publicado en
  ms-nmap |`) usa una redacción ambigua ("publicado en ms-nmap" podría
  sugerir que `ms-nmap` publica `ScanRequest`, cuando en realidad lo
  publica el Gateway y `ms-nmap` solo lo consume — la sección detallada
  inmediatamente debajo, líneas 22–27, sí lo aclara correctamente:
  "Publica: Gateway. Consume: ms-nmap"). No afecta el veredicto porque el
  contenido técnico correcto está presente y sin ambigüedad en el cuerpo
  del documento.

## Tests unitarios (`src/contracts.rs`, sin Docker)

11 tests, todos verdes: payload válido de `ScanRequest` con
`ssh_credentials_ref` de laboratorio; las 3 variantes válidas de
`ScanOutcome` (payloads reales citados de `ms-nmap` para
`completed`/`failed`, shape acordado para `started`); rechazo de
`correlation_id` faltante (con mensaje que identifica el schema
violado), campo extra, `ip` inválida, `completed` con `reason` colado,
`failed` con `result` colado, `status` desconocido. Cubren camino feliz y
camino de error para ambos schemas, tal como exige `docs/verification.md`
Nivel 1.

## Test de integración (`tests/message_contract.rs`, `#[ignore = "requiere Docker"]`)

Ejecutado con Docker real (`cargo test -- --ignored`), ambos tests verdes:

- `scan_request_published_arrives_intact_at_ms_nmap_queue`: valida el
  payload contra el schema antes de publicar, publica en
  `scan.requests`/`scan.request`, y comprueba `assert_eq!` byte a byte
  contra lo recibido en `ms-nmap.scan-requests`.
- `scan_outcome_variants_route_exactly_as_the_topology_declares`: valida
  las 3 variantes contra el schema, publica cada una con su routing key, y
  comprueba que las 3 llegan a `gateway.scan-outcomes` y solo
  `completed`/`failed` llegan a `ms-analisis.scan-outcomes`, con
  `assert_eq!(..., None)` explícito de ausencia de mensajes de más
  (incluida la ausencia positiva de `started` en
  `ms-analisis.scan-outcomes`).

## Credenciales / secretos

`grep` sobre `contracts/`, `src/`, `tests/` confirma que el único valor de
`ssh_credentials_ref` usado en todo el repo es
`"lab-only-not-a-real-secret"` (constante `LAB_SSH_CREDENTIALS_REF`),
claramente identificable como ficticio, nunca el valor real citado en el
test de `ms-nmap` (`"real-broker-secret"`, que el implementer correctamente
evitó reutilizar). No hay credenciales reales en `definitions.json`,
`docker-compose.yml`, fixtures o tests (y `rabbitmq/definitions.json` no
fue tocado por esta feature, ver abajo).

## Alcance de los archivos tocados

`git status`/`git diff --stat` confirman que esta feature solo modificó
`Cargo.toml`/`Cargo.lock` (dependencia `jsonschema` + `thiserror`),
`contracts/README.md`, `src/lib.rs` (una línea, `pub mod contracts;`),
`feature_list.json` (status) y `progress/`, además de crear
`contracts/scan-request.schema.json`, `contracts/scan-outcome.schema.json`,
`src/contracts.rs` y `tests/message_contract.rs`. **No se tocó**
`rabbitmq/definitions.json`, usuarios/permisos, ni la configuración TLS —
correcto para el alcance de esta feature.

## `./init.sh`

Verde de punta a punta: `cargo fmt --check`, `cargo clippy --all-targets --
-D warnings` (sin warnings), `cargo test` (11 unitarios nuevos, 0
fallos), `cargo test -- --ignored` (14 de integración: 12 previos de las
features 2/3 + 2 nuevos de esta feature, todos verdes contra Docker real),
`cargo doc --no-deps` sin errores, `docker compose config` válido.

## Observación menor (no bloqueante)

`src/contracts.rs` líneas 34–41 (función `compile`) usa `panic!()` fuera
de un bloque `#[cfg(test)]` si los schemas embebidos (`include_str!`) no
son JSON/JSON-Schema válidos. `docs/conventions.md` línea 74 dice, sin
excepción textual, "Nada de `unwrap()`/`expect()`/`panic!()` fuera de
tests". Sin embargo `CHECKPOINTS.md` (C3) matiza: "ni `unwrap()`/`panic!()`
fuera de tests **sin justificar**" — y el implementer sí lo justificó
explícitamente en `progress/impl_message_contract.md` (decisión de diseño
3): el panic solo puede dispararse si el propio repo rompe uno de sus dos
archivos `contracts/*.schema.json` (nunca con datos de usuario/mensajes de
RabbitMQ), y ambos tests unitarios y de integración ejercitan `compile()`
en cada corrida de `./init.sh`, por lo que cualquier ruptura se detectaría
de inmediato. La variante `ContractError::InvalidSchema` ya existe sin
usar para un futuro caso de uso runtime (p. ej. compilar un schema
arbitrario). No bloquea la aprobación, pero queda registrado por si una
futura feature reutiliza `compile()` con un schema no controlado por este
repo (en cuyo caso sí debería devolver `Result` en vez de `panic!`).

## Checkpoints (criterios de aceptación de `feature_list.json` id 4)

- C1 (`scan-request.schema.json`, shape exacto, 6 campos required, sin
  propiedades adicionales): [x]
- C2 (`scan-outcome.schema.json`, 3 variantes por `status`, shape interno
  no re-derivado): [x]
- C3 (`started` documentado como pendiente, nunca como ya enviado): [x]
- C4 (README documenta exchange/routing-key/publicador/consumidor
  contrastado contra `rabbitmq/definitions.json`): [x]
- C5 (nota explícita de ausencia de `schema_version`, cambios aditivos): [x]
- C6 (tests unitarios sin Docker: válidos + inválido con mensaje claro): [x]
- C7 (test de integración smoke end-to-end, enrutamiento correcto de las
  3 variantes): [x]
- C8 (`./init.sh` en verde): [x]

## Cambios requeridos

Ninguno. La feature puede marcarse `done`.
