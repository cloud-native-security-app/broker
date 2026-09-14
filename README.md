# broker

Infraestructura y contrato de mensajes del **Broker** de la plataforma de
ciberseguridad blue/red team: la cola de mensajes (**RabbitMQ**) que conecta
`Gateway → ms-nmap` y `ms-nmap → ms-analisis`.

Este repo **no** implementa lógica de negocio de ningún servicio. Provee:

- La **topología** de RabbitMQ (`rabbitmq/definitions.json`): vhost,
  exchanges, colas, bindings, colas de mensajes muertos, y usuarios de
  mínimo privilegio por servicio.
- **TLS obligatorio** para toda conexión — el `ScanRequest` que atraviesa
  este Broker lleva una credencial SSH real (`ssh_credentials_ref`, ya
  implementado en `ms-nmap`), no una referencia opaca.
- El **contrato de mensajes** (`contracts/`): JSON Schema de `ScanRequest` y
  `ScanOutcome`, copiado literalmente de lo que `ms-nmap` ya implementa.
- Un crate Rust de **verificación** (`testcontainers` + `lapin`) que prueba
  la topología, los permisos y el contrato contra un `rabbitmq:management`
  real — nunca contra un broker de producción, nunca con mocks.

**Este repo no publica ninguna librería compartida.** Cada servicio
(`ms-nmap`, `ms-usuarios`, `ms-analisis`, Gateway) implementa su propio
cliente RabbitMQ contra el contrato documentado aquí.

`ms-usuarios`, `ms-nmap`, `ms-analisis` y el Gateway viven en otros repos.

## Desarrollo

El repositorio se desarrolla guiado por agentes de IA sobre un arnés
documental (`AGENTS.md`, `feature_list.json`, `docs/`, `CHECKPOINTS.md`).
Antes de tocar nada, lee `CLAUDE.md`.
