# Verificación — Cómo demostrar que el trabajo funciona

> Regla de oro: **el agente no dice "funciona", lo demuestra**.
> Toda feature termina con evidencia ejecutable, no con afirmaciones.

## Niveles de verificación

### Nivel 0 — Artefactos estructuralmente válidos (obligatorio)

Cada JSON Schema en `contracts/` es JSON válido y JSON Schema válido (lo verifica un test Rust que los carga con el crate `jsonschema`; no hace
falta una herramienta externa).

### Nivel 1 — Tests unitarios (obligatorio si hay lógica pura)

Cualquier función pura del crate de verificación (p. ej. validación de un
payload contra un schema) tiene al menos un test que cubre el camino feliz
y un camino de error.

```bash
cargo test
```

### Nivel 2 — Lints y formato (obligatorio)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Ningún warning de clippy se ignora en silencio; si es un falso positivo, se
documenta con `#[allow(...)]` y un comentario explicando por qué.

### Nivel 3 — Tests de integración con LocalStack

- Contra un contenedor `localstack/localstack:4.4` real, levantado con
  `testcontainers` en el propio test, que crea la topología desde
  `src/topology.rs`. **Nunca** contra una cuenta AWS real.
- Se marcan `#[ignore = "requiere Docker"]` (ver `docs/conventions.md`), se
  ejecutan con `cargo test -- --ignored`, y requieren Docker. Si falla por
  falta de Docker, se documenta como bloqueo en el feature doc de
  `odd/tasks/` — no se reemplaza por un mock.
- Cubren, como mínimo: un payload publicado por SNS llega a SQS íntegro y
  cumple su JSON Schema; `routing_key` y filtros enrutan exactamente (con
  casos negativos); tras 3 recepciones sin borrar el mensaje cae en la DLQ;
  SSE-SQS, retención, visibilidad, redrive y suscripciones raw están presentes.
- LocalStack no aplica cifrado ni políticas: esas piezas se verifican como
  atributos almacenados y, para las políticas (`aws:SecureTransport`, origen
  SNS), por paridad con `terraform/messaging.tf`.

### Nivel 3b — Paridad offline con `infra`

`tests/topology_parity.rs` compara `src/topology.rs` con
`../infra/terraform/messaging.tf` y `../infra/scripts/localstack-init.sh`
(nombres, filtros, `maxReceiveCount`, retenciones, fórmula de visibilidad).
Corre en `cargo test` sin Docker; si `../infra` no existe, avisa y se omite.
Busca literales estables, no hace un parseo completo de HCL o shell.

### Nivel 4 — Contrato extremo a extremo

`tests/sns_sqs_contract.rs` publica un `ScanRequest`, un `ScanCancellation` y
las tres variantes de `ScanOutcome` (con un `ssh_credentials_ref` de
laboratorio, nunca real) y verifica llegada íntegra, atributo `routing_key` y
validación contra los schemas de `contracts/`.

## Anti-patrones (no hacer)

- ❌ "Definí la topología, debería funcionar." → falta test ejecutable
  contra LocalStack.
- ❌ Test que solo verifica que la conexión no falla. → tiene que
  comprobar el contenido concreto (existencia exacta de tópico/cola/
  suscripción, atributos exactos).
- ❌ Probar contra una cuenta AWS de staging o producción, o con
  credenciales reales de ningún tipo.
- ❌ Silenciar un warning de `clippy` con `#[allow(...)]` sin comentario.
- ❌ Marcar una tarea como completada sin pasar `make verify`.
- ❌ Cambiar nombres o parámetros en `infra` sin actualizar `src/topology.rs`
  (y viceversa): la paridad debe seguir verde.

## Verificación final antes de cerrar

```bash
make verify         # fmt, clippy, tests, tests con Docker, doc
```

Si `make verify` está rojo, **no** marques nada como completado. Anota el
bloqueo en el feature doc de `odd/tasks/`.
