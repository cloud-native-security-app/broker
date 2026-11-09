# Convenciones

> Homogeneidad extrema. La IA predice mejor cuando el repositorio se parece
> a sí mismo en todas partes.

## Nombres en SNS y SQS

La mensajería es SNS + SQS (ver `docs/decisions.md` D-13). La fuente de verdad
de nombres y parámetros es `src/topology.rs`; `infra` la implementa
(`terraform/messaging.tf`, `scripts/localstack-init.sh`) y
`tests/topology_parity.rs` comprueba que coinciden.

| Elemento | Convención | Ejemplo |
|----------|------------|---------|
| Tópico SNS | `kebab-case`, `<dominio>-<plural>`, uno por flujo | `scan-requests`, `scan-cancellations`, `scan-outcomes` |
| Cola SQS | `<servicio-consumidor>-<qué-consume>` (SQS no admite puntos en el nombre de forma segura) | `ms-nmap-scan-requests`, `ms-nmap-scan-cancellations`, `gateway-scan-outcomes` |
| Cola de dead-letter | nombre de la cola + `-dlq` | `ms-nmap-scan-requests-dlq` |
| Routing key | atributo de mensaje `String` llamado `routing_key`, `kebab-case` con puntos, describe el evento, no el destinatario | `scan.request`, `scan.outcome.started`, `scan.outcome.completed`, `scan.outcome.failed`, `scan.cancellation` |

- Las suscripciones usan `RawMessageDelivery=true`: el cuerpo SQS es el JSON
  del contrato, sin envoltorio SNS.
- Un consumidor que quiere un subconjunto de eventos usa una `FilterPolicy`
  sobre `routing_key` en su suscripción (p. ej. `gateway-scan-outcomes`:
  prefijo `scan.outcome.`); no se crea un tópico nuevo por eso.
- Todo reintento acotado es `RedrivePolicy.maxReceiveCount=3` hacia la DLQ.
- Cifrado SSE-SQS y retenciones cortas en la cola de requests (1 h) y su DLQ
  (24 h) porque transportan credenciales SSH (`docs/security-scope.md`).
- Los nombres se declaran una vez, en `src/topology.rs`; tests y docs que los
  citan deben coincidir exactamente con ese archivo.

## `contracts/`

- Cada tipo de mensaje tiene su propio archivo JSON Schema
  (`contracts/scan-request.schema.json`, `contracts/scan-outcome.schema.json`),
  usando el mismo formato/estructura de nombres que las claves del JSON
  real (no se traducen a otro idioma ni convención).
- El contrato copia **literalmente** la forma que `ms-nmap` ya implementa —
  antes de tocar un schema, se verifica contra el `domain.rs`/tests de
  `ms-nmap`, no se re-inventa desde cero.
- `contracts/README.md` documenta, por cada mensaje: qué tópico/routing-key
  lo transporta, quién publica, quién consume, y un enlace/referencia al
  módulo de `ms-nmap` que lo define.

## Estilo Rust (crate de verificación)

- **Edition:** 2021 o superior.
- **Formato:** `cargo fmt` (configuración por defecto salvo que se documente
  lo contrario en `rustfmt.toml`).
- **Lints:** `cargo clippy --all-targets -- -D warnings` debe pasar sin
  advertencias (incluye `tests/`).
- **Async:** runtime `tokio` (el SDK de AWS es
  async). Ninguna llamada bloqueante directamente en una tarea async.
- **Errores:** si el crate tiene lógica propia (p. ej. validación de
  schema), tipos de error con `thiserror`, variantes específicas — no
  `String` genérico.
- **Nada de `unwrap()`/`expect()`/`panic!()`** fuera de tests.
- Este crate **no** tiene un binario de producción: no hay `src/main.rs`
  ni imagen Docker de servicio. Si `src/lib.rs` termina teniendo helpers
  públicos usados solo por `tests/`, no hace falta `#![deny(missing_docs)]`
  salvo que se decida exigir rustdoc igual — decídelo en la feature que
  primero cree `src/lib.rs` y documenta la decisión.
- **Tests de integración con IO real:** `testcontainers` levanta
  LocalStack real (`localstack/localstack:4.4`). Nunca mocks de SNS/SQS ni
  cuentas AWS. HTTP en claro solo contra ese LocalStack efímero.
- **Todo test que dependa de Docker se marca**
  `#[ignore = "requiere Docker"]`. `cargo test` (sin flags) corre rápido y
  sin depender de Docker; `cargo test -- --ignored` corre los de
  integración. `make verify` ejecuta ambos.

## Nombres (código Rust)

| Tipo                    | Convención        | Ejemplo               |
|-------------------------|--------------------|------------------------|
| Módulos/archivos        | `snake_case`      | `topology.rs` |
| Tipos/traits            | `PascalCase`      | `QueueSpec` |
| Funciones / variables   | `snake_case`      | `requests_visibility_secs` |
| Constantes              | `UPPER_SNAKE`     | `MAX_RECEIVE_COUNT` |

## Tests

- Tests unitarios: `#[cfg(test)] mod tests` al final del propio archivo,
  para lógica pura (p. ej. validación de JSON Schema). Ve las funciones
  privadas del archivo vía `use super::*`.
- Tests de integración: en `tests/`, contra LocalStack real vía
  `testcontainers` (el test crea la topología que necesita desde
  `src/topology.rs`). Verifican, como mínimo:
  - un payload de ejemplo publicado por SNS llega a SQS como cuerpo JSON
    íntegro que cumple su JSON Schema de `contracts/`;
  - el atributo `routing_key` y los filtros enrutan exactamente como declara
    la topología (incluye casos negativos: lo que no coincide no llega);
  - tras 3 recepciones sin borrar, el mensaje termina en la `-dlq`;
  - los atributos de cifrado, retención, visibilidad y redrive están presentes.
- `tests/topology_parity.rs` es offline y compara con `../infra`; se omite
  (avisando) si ese checkout no existe.
- Nombres de test descriptivos:
  `ms_nmap_user_cannot_publish_to_ms_analisis_queue`.

## Manejo de errores (ejemplo, si aplica)

```rust
#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("el payload no cumple el schema {schema}: {detail}")]
    SchemaViolation { schema: String, detail: String },
}
```

## Comentarios

Por defecto **no** se escriben. Solo se permiten cuando explican un *por qué*
no obvio. Los nombres deben hacer el resto.
