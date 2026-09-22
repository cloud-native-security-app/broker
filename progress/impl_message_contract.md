# Implementación: `message_contract` (id 4)

## Resumen

Se congeló y documentó el contrato de mensajes (`ScanRequest`,
`ScanOutcome` con sus tres variantes `started`/`completed`/`failed`) en
`contracts/`, con validación de JSON Schema (draft 2020-12, crate
`jsonschema`) y tests unitarios + de integración. El shape se tomó
literalmente de `progress/explore_ms_nmap_contract.md` (investigación ya
hecha por el leader leyendo `nmap-service/src/domain.rs` y
`src/messaging/publisher.rs`) — no se re-derivó nada desde cero.

## Archivos creados

- `contracts/scan-request.schema.json` — JSON Schema draft 2020-12 de
  `ScanRequest`: 6 campos (`correlation_id`, `ip`, `network_user`,
  `ssh_credentials_ref`, `has_sudo`, `requested_by`), todos `required`,
  `additionalProperties: false`. `ip` se valida con `anyOf` de dos
  patrones regex (IPv4/IPv6) en vez de `format: ipv4`/`format: ipv6` — ver
  "Decisión de diseño" abajo.
- `contracts/scan-outcome.schema.json` — JSON Schema con `oneOf` de 3
  variantes discriminadas por `status` (`started`/`completed`/`failed`),
  cada una con `additionalProperties: false` y solo sus campos propios.
  `completed.result` referencia `$defs/scanResult` con el shape completo de
  `ScanResult`/`PortFinding`/`VulnFinding` y los 4 enums (`Protocol`,
  `PortState`, `Severity`, `VulnSource`) con los valores string exactos
  citados en `progress/explore_ms_nmap_contract.md` §2.1.
- `src/contracts.rs` — helpers de validación puros (`validate_scan_request`,
  `validate_scan_outcome`), schemas embebidos vía `include_str!` desde
  `contracts/`, compilados una vez por proceso (`OnceLock<Validator>`).
  `ContractError` con `thiserror` (`InvalidSchema`/`SchemaViolation`, sigue
  el patrón de `docs/conventions.md`). Incluye `#[cfg(test)] mod tests` con
  11 tests unitarios (sin Docker): payload válido de `ScanRequest` con
  `ssh_credentials_ref` de laboratorio, los 3 payloads reales/acordados de
  `ScanOutcome`, y varios casos de rechazo (campo faltante, campo extra,
  `ip` inválida, `completed` con `reason` colado, `failed` con `result`
  colado, `status` desconocido) — todos comprueban explícitamente que la
  validación falla (no solo que no paniquea) y que el mensaje de error
  identifica el schema.
- `tests/message_contract.rs` — smoke test end-to-end
  (`#[ignore = "requiere Docker"]`), reutiliza `tests/common/mod.rs`:
  1. Publica un `ScanRequest` de ejemplo válido (validado primero contra el
     schema) en `scan.requests`/`scan.request` y verifica que llega
     **íntegro** (`assert_eq!` byte a byte tras deserializar) a
     `ms-nmap.scan-requests`.
  2. Publica los 3 `ScanOutcome` de ejemplo (payloads reales citados en
     `progress/explore_ms_nmap_contract.md` §2 para `completed`/`failed`,
     shape acordado para `started`) con sus routing keys, y verifica que
     las 3 llegan a `gateway.scan-outcomes` pero SOLO `completed`/`failed`
     llegan a `ms-analisis.scan-outcomes` (con `assert_eq!(..., None)`
     explícito de que no llegan mensajes de más, incluida la ausencia
     positiva de `started` en `ms-analisis.scan-outcomes`).

## Archivos modificados

- `contracts/README.md` — reescrito desde el placeholder de `scaffolding`:
  tabla de archivos, exchange/routing-key/publicador/consumidor por
  mensaje (contrastado contra `rabbitmq/definitions.json` real: `gateway`
  escribe `scan.requests`; `ms-nmap` consume `ms-nmap.scan-requests` y
  publica en `scan.outcomes`; `ms-analisis` consume SOLO
  `scan.outcome.completed`/`failed`; Gateway consume las 3 vía
  `scan.outcome.#`), sección dedicada a `started` como "contrato acordado,
  publicación PENDIENTE en ms-nmap" (nunca afirma que ya se envía), nota
  sobre por qué `ip`/`host` usan `pattern` en vez de `format`, nota sobre
  por qué `cpes`/`references`/`source` son `required` en el mensaje del
  Broker aunque `ms-nmap` los tolere ausentes al leer de su propia base de
  datos, y la nota explícita de "sin `schema_version`, cambios deben ser
  aditivos".
- `src/lib.rs` — añade `pub mod contracts;`.
- `Cargo.toml` — añade `jsonschema = { version = "0.56", default-features
  = false }` (sin features `reqwest`/`resolve-http`/`tls-*`: nuestros
  schemas son autocontenidos, sin `$ref` externos, así que no hace falta
  resolución remota — evita además introducir un tercer proveedor
  criptográfico de `rustls` en el binario de tests, ver el comentario ya
  existente en `Cargo.toml` sobre `ring`/`aws_lc_rs`) y
  `thiserror = "2.0"` (no estaba como dependencia todavía; lo pide
  `docs/conventions.md` para `ContractError`).
- `Cargo.lock` — actualizado por `cargo build`/`cargo add --dry-run` +
  build real.
- `progress/current.md` — se añadió la sección "Implementer:
  message_contract (id 4)" con el plan de esta sesión (el resto del
  archivo, incluida la bitácora del leader, se dejó intacta para que el
  reviewer/leader tengan el hilo completo).

## Decisiones de diseño (dentro del margen del brief)

1. **`ip`/`host` con `pattern` en vez de `format: ipv4`/`format: ipv6`.**
   El brief lo permitía explícitamente ("o un patrón regex si prefieres no
   depender de que el validador soporte esos formats"). Se confirmó
   además que el crate `jsonschema` 0.56 **no** valida `format` por
   defecto en draft 2020-12 (es annotation-only salvo que se llame
   `should_validate_formats(true)` explícitamente, ver
   `jsonschema-0.56.0/src/lib.rs`) — si se hubiera usado `oneOf` con
   `format: ipv4`/`format: ipv6` sin activar esa opción, ambas ramas
   habrían sido efectivamente `{"type": "string"}` sin diferencia, y
   `oneOf` habría rechazado **cualquier** IP válida por matchear las dos
   ramas a la vez. Usar `pattern` es determinista independientemente del
   validador/opciones de turno.
2. **`jsonschema` con `default-features = false`.** Las features por
   defecto (`reqwest`, `resolve-http`, `tls-aws-lc-rs`, `idna`) son para
   resolución de `$ref` remotos e IDN — no las usamos (nuestros schemas
   solo tienen `$ref` internos a `#/$defs/...`) y evitan pull-in
   innecesario de otro stack TLS.
3. **`ContractError` con `thiserror`**, dos variantes
   (`InvalidSchema`/`SchemaViolation`) — `InvalidSchema` queda para un
   futuro caso de uso (p. ej. si se expone compilar un schema arbitrario
   en runtime); hoy los schemas embebidos se compilan con `panic!` si
   están rotos (error de programación del propio repo, no un caso de
   `Result` esperado en producción — coherente con "nunca `unwrap()` fuera
   de tests" de `docs/conventions.md`: aquí sí hace panic pero en
   inicialización de una constante estática de compilación, no en un
   camino de datos de usuario).
4. **`cpes`/`references`/`source` como `required`** en
   `scan-outcome.schema.json`, siguiendo textualmente la recomendación de
   `progress/explore_ms_nmap_contract.md` §2.1 (la tolerancia
   `#[serde(default...)]` de `ms-nmap` es para deserializar documentos
   viejos de su propia base de datos, no para lo que publica en el
   Broker). Documentado explícitamente en `contracts/README.md`.
5. **No se implementó `scan-cancellation.schema.json`** — es la feature
   `cancellation_contract` (id 5), fuera de alcance de esta sesión.

## Resultado de `./init.sh`

Verde de punta a punta, incluida la sección 5 completa:

- `cargo fmt --check` — sin diferencias.
- `cargo clippy --all-targets -- -D warnings` — sin warnings.
- `cargo test` (sin Docker) — **11 tests unitarios nuevos** en
  `src/contracts.rs` (antes: 0 tests unitarios en el crate) + los tests de
  integración ya existentes/nuevos aparecen como `ignored` en esta pasada.
- `cargo test -- --ignored` (con Docker) — **14 tests de integración**
  pasan: los 12 ya existentes de las features 2/3
  (`outcome_routing.rs` x2, `permissions.rs` x6, `retry_delivery_limit.rs`
  x1, `tls.rs` x2, `topology_exists.rs` x1) + los **2 nuevos** de
  `tests/message_contract.rs`.
- `cargo doc --no-deps` — sin errores.
- `docker compose config` — válido (sin cambios en `docker-compose.yml`).

Total: 11 tests unitarios + 14 tests de integración = 25 tests, todos en
verde.

## Desviaciones del brief

Ninguna relevante. Se usó `pattern` en vez de `format` para `ip`/`host`
(explícitamente permitido por el brief, ver decisión 1 arriba) y se añadió
`thiserror` como dependencia nueva (no mencionada explícitamente en el
brief pero exigida por `docs/conventions.md` para tipos de error propios).

## Pendiente / bloqueos

Ninguno. `./init.sh` en verde, feature lista para revisión por el
`reviewer`. No se marcó `status: "done"` en `feature_list.json` — eso lo
hace el leader tras la aprobación del reviewer, según el protocolo.

## Cierre de sesión (2026-09-17)

Reviewer emitió veredicto **APPROVED** sin cambios requeridos
(`progress/review_message_contract.md`), tras contrastar campo por campo
ambos JSON Schema contra el código fuente real de `ms-nmap`
(`domain.rs`, `publisher.rs`). Dos observaciones menores no bloqueantes,
sin acción requerida: redacción ambigua en `contracts/README.md` línea 16
(el contenido técnico correcto ya está aclarado en el cuerpo del
documento) y el uso justificado de `panic!()` en
`src/contracts.rs::compile` (aceptable mientras solo compile los 2
schemas propios del repo).

Pasos de cierre ejecutados siguiendo `AGENTS.md` §5:

1. `./init.sh` re-ejecutado de punta a punta (incluye `--ignored` con
   Docker real): verde completo — **11 tests unitarios** (`src/contracts.rs`,
   sin Docker) + **14 tests de integración** (`testcontainers`+`lapin`
   contra `rabbitmq:4.3.5-management` real: `message_contract.rs` x2,
   `outcome_routing.rs` x2, `permissions.rs` x6, `retry_delivery_limit.rs`
   x1, `tls.rs` x2, `topology_exists.rs` x1), `cargo fmt --check`,
   `cargo clippy --all-targets -- -D warnings` y `cargo doc --no-deps`
   sin errores/warnings, `docker compose config` válido.
2. `feature_list.json` id 4 (`message_contract`) → `status: "done"`.
3. Resumen movido de `progress/current.md` al final de
   `progress/history.md` (sección "2026-09-17 — Feature 4
   `message_contract` — DONE"), documentando qué se creó (2 JSON Schema,
   `contracts/README.md`, `src/contracts.rs` con validación vía
   `jsonschema`, test de integración end-to-end del enrutamiento) y el
   veredicto del reviewer.
4. `progress/current.md` vaciado, dejando solo la plantilla original
   (`Feature en curso: _ninguna_`).
5. Verificado que no quedan contenedores/volúmenes Docker huérfanos de
   este repo: `docker ps -a` solo muestra contenedores de otros proyectos
   ajenos (ya detenidos antes de esta sesión); `docker compose ps -a` no
   muestra servicios levantados (testcontainers se limpió solo tras cada
   test); no fue necesario `docker compose down -v` porque no se levantó
   ningún stack persistente en esta sesión. Sin archivos temporales
   sueltos.

Feature `message_contract` (id 4) cerrada.
