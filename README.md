# broker

Contrato de mensajes y topología de mensajería de la plataforma de
ciberseguridad blue/red team: **AWS SNS + SQS** conectando
`Gateway → ms-nmap` y `ms-nmap → Gateway` (y, cuando exista, `ms-analisis`).
RabbitMQ fue retirado (`docs/decisions.md`, D-13).

Este repo **no** implementa lógica de negocio de ningún servicio ni es un
broker. Provee:

- El **contrato de mensajes** (`contracts/`): JSON Schema de `ScanRequest`,
  `ScanOutcome` y `ScanCancellation`, copiado literalmente de lo que
  `ms-nmap` ya implementa.
- La **topología esperada** como datos puros (`src/topology.rs`): tópicos SNS,
  colas SQS con su `-dlq`, `maxReceiveCount=3`, filtros por `routing_key`,
  visibilidad y retención. Es el diseño firmado en
  `odd/tasks/sqs-sns-migration.md`.
- Un crate Rust de **verificación** (`testcontainers` + LocalStack + SDK de
  AWS) que prueba contrato, enrutamiento, filtros, DLQ y atributos de
  cifrado/retención contra un LocalStack real, y compara la topología con
  `infra` (offline). Nunca contra AWS real, nunca con mocks.

**Este repo no publica ninguna librería compartida.** Cada servicio
(`gateway`, `nmap-service`) implementa su propio cliente SNS/SQS contra el
contrato documentado aquí. `ms-usuarios` y `ms-analisis` viven en otros repos.

## Desarrollo local

La mensajería local (LocalStack con la topología completa) vive en el repo
`infra`: `scripts/localstack-init.sh` y el servicio `localstack` de su
`docker-compose.yml`. Este repo ya no tiene compose propio; sus tests levantan
su propio LocalStack efímero.

## Verificación

Toda la verificación se ejecuta con `make verify` (requiere Rust estable y
Docker). Targets individuales:

| Target | Comando |
|---|---|
| `make fmt` | `cargo fmt --check` |
| `make lint` | `cargo clippy --all-targets -- -D warnings` |
| `make test` | `cargo test` (unitarios y paridad offline con `../infra`, sin Docker) |
| `make test-integration` | `cargo test -- --ignored` (LocalStack vía testcontainers, requiere Docker) |
| `make doc` | `cargo doc --no-deps` |

Los tests de integración usan `localstack/localstack:4.4`; si Docker no está
disponible se reporta como bloqueo, nunca se reemplazan por mocks. Ver
`docs/verification.md`.

## Documentación

- `docs/architecture.md`: alcance, topología y estándar de calidad.
- `docs/conventions.md`, `docs/security-scope.md`, `docs/verification.md`.
- `docs/decisions.md`: decisiones de diseño y su contexto.

## Desarrollo asistido por IA

Este repo usa **gentle-ai**: cada funcionalidad sustancial se planifica en un
feature doc de Organic Driven Development bajo `odd/tasks/` (por ejemplo
`odd/tasks/sqs-sns-migration.md`; `amazon-mq-migration.md` quedó reemplazado),
con una copia de recuperación en Engram. El trabajo se hace en ramas, con
Conventional Commits y verificando con `make verify` antes de cerrar cada
tarea.

## Despliegue

Este repo no produce ninguna imagen ni binario de producción. La topología se
despliega desde `infra` (`terraform/messaging.tf`) y la paridad con
`src/topology.rs` la comprueba `tests/topology_parity.rs`. Cada servicio recibe
los ARN de tópico y URL de cola por variables de entorno definidas en `infra`;
las credenciales nunca viven en este repo (`docs/security-scope.md`).
