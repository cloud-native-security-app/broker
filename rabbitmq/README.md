# rabbitmq/

Este directorio contendrá `definitions.json`: la fuente de verdad de la
topología real de RabbitMQ (vhost, exchanges, colas, bindings, colas de
dead-letter, usuarios de servicio y sus permisos), en el formato nativo que
RabbitMQ importa vía `management.load_definitions` /
`RABBITMQ_LOAD_DEFINITIONS`.

Se puebla en:

- **Feature `topology_definition`** (id 2): vhost `security-app`, exchanges
  `scan.requests`/`scan.outcomes` con sus dead-letter, colas
  `ms-nmap.scan-requests`, `ms-analisis.scan-outcomes`,
  `gateway.scan-outcomes` con reintentos acotados antes de dead-letter, y
  los usuarios de mínimo privilegio `gateway`, `ms-nmap`, `ms-analisis`.
- **Feature `cancellation_contract`** (id 5): exchange `scan.cancellations`
  y cola `ms-nmap.scan-cancellations`, ampliando permisos de `gateway` y
  `ms-nmap`.

Vacío por ahora (feature `scaffolding`, id 1) — no hay topología real
todavía. Ver `docs/architecture.md` y `docs/conventions.md` para las
convenciones de nombres antes de escribir `definitions.json`.
