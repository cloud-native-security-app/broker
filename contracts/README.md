# contracts/

Este directorio contendrá el JSON Schema (y su documentación) de cada tipo
de mensaje que viaja por el Broker, copiado literalmente de la forma que
`ms-nmap` ya implementa y testea — no se re-inventa desde cero.

Se puebla en:

- **Feature `message_contract`** (id 4): `scan-request.schema.json`,
  `scan-outcome.schema.json` (variantes `started`/`completed`/`failed`), y
  un `README.md` propio documentando qué exchange/routing-key transporta
  cada mensaje y quién publica/consume.
- **Feature `cancellation_contract`** (id 5): `scan-cancellation.schema.json`.

Vacío por ahora (feature `scaffolding`, id 1) — no hay contrato de mensajes
todavía. Ver `docs/architecture.md` (sección "Contrato de mensajes") y
`docs/conventions.md` antes de escribir un schema.
