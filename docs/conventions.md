# Convenciones

> Homogeneidad extrema. La IA predice mejor cuando el repositorio se parece
> a sí mismo en todas partes.

## Nombres en RabbitMQ

| Elemento | Convención | Ejemplo |
|----------|------------|---------|
| Vhost | `kebab-case`, uno para toda la plataforma salvo razón concreta | `security-app` |
| Exchange | `kebab-case`, tipo `topic` salvo razón concreta, singular del dominio + `.` + acción | `scan.requests`, `scan.outcomes` |
| Exchange de dead-letter | mismo nombre + `.dlx` | `scan.requests.dlx` |
| Cola | `<servicio-consumidor>.<qué-consume>` | `ms-nmap.scan-requests`, `ms-analisis.scan-outcomes` |
| Cola de dead-letter | mismo nombre + `.dlq` | `ms-nmap.scan-requests.dlq` |
| Routing key | `kebab-case` con puntos, describe el evento, no el destinatario | `scan.request`, `scan.outcome.completed`, `scan.outcome.failed` |
| Usuario de RabbitMQ | igual al nombre del servicio | `gateway`, `ms-nmap`, `ms-analisis` |

- Un usuario **solo** tiene permisos (`configure`/`write`/`read`, por regex)
  sobre los exchanges/colas que su servicio necesita — nunca acceso amplio
  "por si acaso". Ver `docs/security-scope.md`.
- Los nombres se declaran una sola vez, en `rabbitmq/definitions.json`; no
  se hardcodean strings sueltas en tests o docs sin que coincidan
  exactamente con ese archivo (si se referencian ahí, es la fuente de
  verdad).

## `rabbitmq/definitions.json`

- Es el formato nativo que RabbitMQ importa vía
  `management.load_definitions` (JSON, con las claves `vhosts`,
  `exchanges`, `queues`, `bindings`, `users`, `permissions`, `policies`).
  No se escribe a mano un formato propio: se sigue el schema real que
  exporta/importa RabbitMQ.
- Las políticas de dead-lettering (`x-dead-letter-exchange`,
  `x-dead-letter-routing-key`) y de TTL se declaran como argumentos de la
  cola (`arguments`) o como `policies`, nunca implícitas en el código de
  los tests.
- Ningún valor de este archivo es una credencial real (ver
  `docs/security-scope.md`): las contraseñas de los usuarios de
  `definitions.json` son de laboratorio, nunca secretos de producción.

## `contracts/`

- Cada tipo de mensaje tiene su propio archivo JSON Schema
  (`contracts/scan-request.schema.json`, `contracts/scan-outcome.schema.json`),
  usando el mismo formato/estructura de nombres que las claves del JSON
  real (no se traducen a otro idioma ni convención).
- El contrato copia **literalmente** la forma que `ms-nmap` ya implementa —
  antes de tocar un schema, se verifica contra el `domain.rs`/tests de
  `ms-nmap`, no se re-inventa desde cero.
- `contracts/README.md` documenta, por cada mensaje: qué exchange/routing-key
  lo transporta, quién publica, quién consume, y un enlace/referencia al
  módulo de `ms-nmap` que lo define.

## Estilo Rust (crate de verificación)

- **Edition:** 2021 o superior.
- **Formato:** `cargo fmt` (configuración por defecto salvo que se documente
  lo contrario en `rustfmt.toml`).
- **Lints:** `cargo clippy --all-targets -- -D warnings` debe pasar sin
  advertencias (incluye `tests/`).
- **Async:** runtime `tokio` (los clientes RabbitMQ como `lapin` son
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
  `rabbitmq:<tag>-management` real. Nunca mocks de RabbitMQ.
- **Todo test que dependa de Docker se marca**
  `#[ignore = "requiere Docker"]`. `cargo test` (sin flags) corre rápido y
  sin depender de Docker; `cargo test -- --ignored` corre los de
  integración. `init.sh` ejecuta ambos.

## Nombres (código Rust)

| Tipo                    | Convención        | Ejemplo               |
|-------------------------|--------------------|------------------------|
| Módulos/archivos        | `snake_case`      | `topology.rs` |
| Tipos/traits            | `PascalCase`      | `TopologyError` |
| Funciones / variables   | `snake_case`      | `assert_permissions` |
| Constantes              | `UPPER_SNAKE`     | `RABBITMQ_IMAGE` |

## Tests

- Tests unitarios: `#[cfg(test)] mod tests` al final del propio archivo,
  para lógica pura (p. ej. validación de JSON Schema). Ve las funciones
  privadas del archivo vía `use super::*`.
- Tests de integración: en `tests/`, contra `rabbitmq:management` real vía
  `testcontainers`. Verifican, como mínimo:
  - la topología declarada en `definitions.json` existe tal cual (vhost,
    exchanges, colas, bindings);
  - cada usuario puede hacer **solo** lo que sus permisos declaran —
    incluye un caso **positivo** (puede publicar/consumir lo suyo) y un
    caso **negativo** (acceso denegado a un exchange/cola ajeno);
  - el dead-lettering funciona (un mensaje que agota reintentos/TTL
    termina en la `.dlq` correspondiente);
  - un payload de ejemplo válido/ inválido contra cada JSON Schema de
    `contracts/`.
- Nombres de test descriptivos:
  `ms_nmap_user_cannot_publish_to_ms_analisis_queue`.

## Manejo de errores (ejemplo, si aplica)

```rust
#[derive(Debug, thiserror::Error)]
pub enum TopologyError {
    #[error("no se pudo conectar con RabbitMQ: {0}")]
    ConnectionFailed(String),
    #[error("el payload no cumple el schema {schema}: {detail}")]
    SchemaViolation { schema: String, detail: String },
}
```

## Comentarios

Por defecto **no** se escriben. Solo se permiten cuando explican un *por qué*
no obvio. Los nombres deben hacer el resto.
