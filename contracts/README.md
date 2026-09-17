# contracts/

JSON Schema (draft 2020-12) del contrato de mensajes del Broker, más esta
documentación de qué exchange/routing-key transporta cada mensaje y quién
publica/consume. El contrato **se copia literalmente** de lo que `ms-nmap`
ya implementa y testea (`src/domain.rs`, `src/messaging/publisher.rs`), no
se re-deriva desde cero — ver
[`progress/explore_ms_nmap_contract.md`](../progress/explore_ms_nmap_contract.md)
(investigación del leader, con citas literales/líneas exactas del repo
hermano `nmap-service`) para el rastro de esa verificación.

## Archivos

| Archivo | Mensaje | Estado |
|---|---|---|
| [`scan-request.schema.json`](./scan-request.schema.json) | `ScanRequest` | Implementado y publicado en `ms-nmap` |
| [`scan-outcome.schema.json`](./scan-outcome.schema.json) | `ScanOutcome` (`started`/`completed`/`failed`) | `completed`/`failed` implementados; `started` **PENDIENTE** (ver abajo) |

`scan-cancellation.schema.json` (`ScanCancellation`) se define en la feature
`cancellation_contract` (id 5), fuera de alcance de esta feature.

## `ScanRequest`

- **Exchange:** `scan.requests` (topic).
- **Routing key:** `scan.request`.
- **Publica:** Gateway.
- **Consume:** `ms-nmap` (cola `ms-nmap.scan-requests`).
- **Fuente:** `nmap-service/src/domain.rs` líneas 133–156 (struct `ScanRequest`),
  contrastado en `progress/explore_ms_nmap_contract.md` §1.

Los 6 campos son `required` y el schema usa `additionalProperties: false` —
ningún campo extra pasa silenciosamente.

`ssh_credentials_ref` es la credencial SSH **real** con la que `ms-nmap` se
autentica en el objetivo, no una referencia opaca — ver
`docs/security-scope.md`. El schema la tipa como `string` sin más: no hay
forma de que JSON Schema distinga "es un secreto real" de cualquier otro
string; la responsabilidad de no loggear el cuerpo del mensaje es de cada
servicio (ver `docs/security-scope.md` §"Retención y logging"). Ningún
ejemplo/fixture de este repo usa una credencial real — solo valores de
laboratorio, identificables a simple vista (p. ej.
`"lab-only-not-a-real-secret"`).

`ip` valida con un patrón regex (`anyOf` IPv4/IPv6) en vez de
`format: ipv4`/`format: ipv6`, para no depender de que el validador de turno
implemente esas format-assertions (en JSON Schema 2020-12, `format` es
annotation-only por defecto salvo que el validador adopte el vocabulario de
aserción explícitamente — usar `pattern` es determinista en cualquier
implementación conforme al draft).

## `ScanOutcome`

- **Exchange:** `scan.outcomes` (topic), internamente tageado por `status`.
- **Routing keys:** `scan.outcome.started`, `scan.outcome.completed`,
  `scan.outcome.failed`.
- **Publica:** `ms-nmap`.
- **Consume, con bindings DIFERENCIADOS** (ver `rabbitmq/definitions.json`,
  feature `topology_definition`):
  - `ms-analisis` (cola `ms-analisis.scan-outcomes`) — bindings SOLO a
    `scan.outcome.completed` y `scan.outcome.failed`. `ms-analisis` nunca
    recibe `started`: no le interesa el estado intermedio "en progreso", solo
    desenlaces finales.
  - Gateway (cola `gateway.scan-outcomes`) — binding a `scan.outcome.#`
    (las tres variantes), para reflejar
    PENDIENTE/EN_PROGRESO/COMPLETADO/FALLIDO en el frontend en tiempo real
    (RF-07/RF-08).
- **Fuente `completed`/`failed`:** `nmap-service/src/messaging/publisher.rs`
  líneas 62–91 (enum `ScanOutcome`, `#[serde(tag = "status", rename_all =
  "snake_case")]`), contrastado en `progress/explore_ms_nmap_contract.md` §2.
- **Fuente `ScanResult` (anidado en `completed`):**
  `nmap-service/src/domain.rs` líneas 213–298, contrastado en
  `progress/explore_ms_nmap_contract.md` §2.1.

### Variante `started` — contrato acordado, publicación PENDIENTE en `ms-nmap`

`{"status": "started", "correlation_id": "..."}` es el contrato **acordado**
para RF-07/RF-08 (que el Gateway pueda reflejar el estado EN_PROGRESO), pero
**no existe todavía en el código real de `ms-nmap`** — el enum `ScanOutcome`
de `nmap-service/src/messaging/publisher.rs` solo tiene las variantes
`Completed`/`Failed` a día de hoy (confirmado en
`progress/explore_ms_nmap_contract.md` §2, líneas 77–82). Este repo define el
canal (exchange, routing key, binding en `gateway.scan-outcomes`) y el
schema para que `ms-nmap` pueda implementarla sin renegociar el contrato,
pero **no afirma que `ms-nmap` la publique ya**. Implementarla y publicarla
es trabajo pendiente en el repo `nmap-service`, fuera de alcance de este
repo.

### `completed` vs `failed`: sin campos cruzados

Confirmado por los tests de `ms-nmap` citados en
`progress/explore_ms_nmap_contract.md` §2 (`json.get("reason").is_none()`
para `completed`, `json.get("result").is_none()` para `failed`): cada
variante tiene `additionalProperties: false` y solo sus propios campos — un
mensaje `completed` con un `reason` colado (o un `failed` con `result`) es
inválido contra el schema.

### `ScanResult` — nota sobre campos con `#[serde(default...)]` en `ms-nmap`

`ms-nmap` tolera, **al deserializar** documentos viejos de su propia base de
datos (no mensajes del Broker), que `PortFinding.cpes`,
`VulnFinding.references` y `VulnFinding.source` falten (ver
`progress/explore_ms_nmap_contract.md` §2.1). Esta tolerancia es una
decisión de persistencia interna de `ms-nmap`, no del contrato de
mensajería: el mensaje `ScanOutcome.completed` que `ms-nmap` **publica** en
el Broker siempre incluye esos campos (aunque sea con `[]` o el valor por
defecto). Por eso `scan-outcome.schema.json` los marca como `required` en
`PortFinding`/`VulnFinding` — el contrato de mensajería no es más laxo que
lo que `ms-nmap` efectivamente emite. Si en el futuro se decide relajar esto
(permitir que falten en el mensaje del Broker), es un cambio de contrato a
discutir explícitamente, no una corrección silenciosa de este README.

## Sin `schema_version` (nota de versionado)

Ningún mensaje de este contrato lleva un campo de versión de schema
(`schema_version` u otro mecanismo) — ni `ScanRequest` ni `ScanOutcome` lo
tienen en el código real de `ms-nmap`, y no se inventa uno aquí sin
discutirlo antes con el usuario (ver `docs/architecture.md`, sección "Qué NO
hacer"). Mientras no exista una política de versionado acordada, **cualquier
cambio de formato de estos mensajes debe ser aditivo** (campos nuevos
opcionales, nunca renombrar/eliminar/cambiar el tipo de un campo existente
sin coordinar el cambio con `ms-nmap` y el resto de consumidores — Gateway,
`ms-analisis`). Un cambio no aditivo requiere una decisión explícita de
versionado antes de tocar estos schemas.

## Ejemplos de referencia

Los ejemplos usados en los tests de este repo
(`src/contracts.rs`, `tests/message_contract.rs`) son literalmente los
citados en `progress/explore_ms_nmap_contract.md` (extraídos de los propios
tests de `ms-nmap`) para `completed`/`failed`, y el shape acordado en
`feature_list.json` para `started`. Ningún ejemplo usa una credencial SSH
real: el `ssh_credentials_ref` de los payloads de ejemplo es siempre un
valor de laboratorio explícitamente ficticio (p. ej.
`"lab-only-not-a-real-secret"`), nunca el que aparece en el test real de
`ms-nmap` (que si es real, ver nota de seguridad arriba).
