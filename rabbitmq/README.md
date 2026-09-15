# rabbitmq/

Fuente de verdad de la topología real de RabbitMQ: vhost, exchanges,
colas, bindings, colas de dead-letter, usuarios de servicio y sus
permisos. RabbitMQ la carga al arrancar — ver "Carga de la topología"
abajo.

## Archivos

- **`definitions.json`** — topología completa en el formato nativo que
  RabbitMQ importa (`vhosts`, `exchanges`, `queues`, `bindings`, `users`,
  `permissions`). Es la única fuente de verdad: nombres, routing keys y
  permisos no se hardcodean por duplicado en otro sitio salvo que
  referencien exactamente estos valores (p. ej. los tests de `tests/`).
- **`rabbitmq.conf`** — le dice al nodo dónde está `definitions.json`
  (`load_definitions`).

## Carga de la topología

La imagen oficial `rabbitmq:<tag>-management` (`docker-library/rabbitmq`)
**no** soporta la variable de entorno `RABBITMQ_LOAD_DEFINITIONS` (esa es
específica de la imagen Bitnami). El mecanismo correcto, usado por este
repo, es:

1. Montar `rabbitmq/definitions.json` en `/etc/rabbitmq/definitions.json`.
2. Montar `rabbitmq/rabbitmq.conf` en `/etc/rabbitmq/rabbitmq.conf`, con la
   línea `load_definitions = /etc/rabbitmq/definitions.json`.

Se usa la clave núcleo `load_definitions` (sin el prefijo `management.`)
porque un issue documentado de `docker-library/rabbitmq` (#428) reporta que
`management.load_definitions` falla en cargar correctamente en el
*primer* arranque del contenedor, mientras que `load_definitions` (soportada
desde RabbitMQ 3.8.2+) sí funciona de forma fiable. Ver
`progress/explore_definitions_format.md` §0 para el detalle y las fuentes.

`docker-compose.yml` monta ambos archivos; los tests de integración de
`tests/` (vía `testcontainers`) montan los mismos dos archivos en sus
propios contenedores efímeros, para verificar exactamente la misma
topología que corre en local.

## Topología (feature `topology_definition`)

Vhost dedicado: **`security-app`**.

| Exchange | Tipo | Dead-letter exchange |
|---|---|---|
| `scan.requests` | `topic` | `scan.requests.dlx` (`direct`) |
| `scan.outcomes` | `topic` | `scan.outcomes.dlx` (`direct`) |

| Cola | Bindeada a | Routing key(s) | Dead-letter queue |
|---|---|---|---|
| `ms-nmap.scan-requests` | `scan.requests` | `scan.request` | `ms-nmap.scan-requests.dlq` |
| `ms-analisis.scan-outcomes` | `scan.outcomes` | `scan.outcome.completed`, `scan.outcome.failed` (**nunca** `scan.outcome.started`) | `ms-analisis.scan-outcomes.dlq` |
| `gateway.scan-outcomes` | `scan.outcomes` | `scan.outcome.#` (started, completed, failed) | `gateway.scan-outcomes.dlq` |

`gateway.scan-outcomes` es lo que permite al Gateway notificar al frontend
en tiempo real (RF-08) y reflejar los cuatro estados PENDIENTE/EN_PROGRESO/
COMPLETADO/FALLIDO (RF-07); `ms-analisis` solo necesita los desenlaces
terminales (`completed`/`failed`), nunca `started`.

Los exchanges `*.dlx` son `direct` (no `topic`): cada uno solo necesita
enrutar hacia un destino final fijo por routing key exacta (una `.dlq` por
routing key de dead-letter), no hace falta el matching por patrones de un
`topic` exchange.

## Reintentos acotados antes de `.dlq` (RNF-06)

Cada cola principal (`ms-nmap.scan-requests`, `ms-analisis.scan-outcomes`,
`gateway.scan-outcomes`) es una **cola quorum** (`"x-queue-type": "quorum"`)
con:

```json
"x-delivery-limit": 3,
"x-dead-letter-exchange": "<su .dlx>",
"x-dead-letter-routing-key": "<routing key exclusiva de esa cola>"
```

**Por qué `x-delivery-limit` (mecanismo nativo de colas quorum) y no el
patrón clásico de "cola de retry intermedia con TTL"**: con
`x-delivery-limit`, RabbitMQ mismo cuenta las redeliveries y decide el
corte hacia la `.dlq` — no hace falta ningún exchange/cola de retry
adicional, ni lógica de "consumidor simulado" en el crate de verificación
(este repo no implementa consumidores de producción, ver
`docs/architecture.md`). Justificación completa, comparación con el patrón
clásico y sintaxis en `progress/explore_retry_pattern.md`.

- **N = 3** reintentos: un mensaje se entrega hasta 4 veces en total (1
  entrega original + 3 redeliveries) antes de dead-letrarse
  automáticamente con `reason: "delivery_limit"` en el header `x-death`.
  Valor de laboratorio razonable ("3 strikes"): absorbe fallos transitorios
  cortos sin enmascarar un fallo persistente, y es mucho menor que el
  default de RabbitMQ 4.0+ (20), que dejaría un mensaje envenenado
  reintentando silenciosamente muchas veces antes de ser visible en la
  `.dlq`.
- **Sin TTL de por medio**: `x-delivery-limit` no introduce demora entre
  reintentos (la redelivery es inmediata); no hay un `x-message-ttl` que
  fijar. Si un consumidor real (`ms-nmap`, etc.) quisiera espaciar sus
  reintentos, eso es una decisión de *backoff en el cliente*
  (p. ej. 500ms–2000ms de espera antes de rechazar), no de la topología del
  Broker — este repo no la impone ni la puede verificar (no hay consumidor
  real aquí).

**Nota empírica importante para quien implemente el consumidor real**
(confirmada contra RabbitMQ 4.3.5 real, ver `tests/retry_delivery_limit.rs`):
el contador que compara contra `x-delivery-limit` es `x-delivery-count`, que
**solo se incrementa con `basic.reject`** (el método AMQP 0-9-1 estándar de
un solo mensaje). `basic.nack` con `requeue=true` solo incrementó
`x-acquired-count` (un contador informativo) en las pruebas de este repo y
**nunca** disparó el dead-lettering. Un consumidor real que dependa de este
mecanismo debe usar `basic.reject`, no `basic.nack`.

## Usuarios y permisos de mínimo privilegio

Un usuario de RabbitMQ por servicio productor/consumidor, nunca uno
compartido; permisos `configure`/`write`/`read` acotados por regex
**solo** a los exchanges/colas que cada servicio necesita (ver
`docs/security-scope.md`):

| Usuario | `configure` | `write` | `read` |
|---|---|---|---|
| `lab-admin` (administrador) | `.*` | `.*` | `.*` |
| `gateway` | `^$` | `^scan\.requests$` | `^gateway\.scan-outcomes$` |
| `ms-nmap` | `^$` | `^scan\.outcomes$` | `^ms-nmap\.scan-requests$` |
| `ms-analisis` | `^$` | `^$` | `^ms-analisis\.scan-outcomes$` |

Ningún usuario de servicio puede leer la cola de otro servicio ni escribir
fuera de su exchange: `gateway` no puede leer `ms-nmap.scan-requests` ni
`ms-analisis.scan-outcomes`; `ms-nmap` no puede escribir en `scan.requests`
ni leer las colas de `gateway`/`ms-analisis`; `ms-analisis` no puede
escribir nada. `configure: "^$"` en los tres usuarios de servicio: ninguno
declara/borra topología — eso solo lo hace `definitions.json` (vía el
usuario `lab-admin`).

## Credenciales de laboratorio (NUNCA reales)

Todas las contraseñas de abajo son de laboratorio, usadas únicamente por
`docker-compose.yml` local y por los tests de integración de `tests/`
(vía `testcontainers`). **Nunca** son válidas para un entorno real — ver
`docs/security-scope.md` y la feature `deployment_docs`.

| Usuario | Contraseña de laboratorio |
|---|---|
| `lab-admin` | `lab-only-not-a-real-secret` |
| `gateway` | `lab-only-not-a-real-secret-gateway` |
| `ms-nmap` | `lab-only-not-a-real-secret-ms-nmap` |
| `ms-analisis` | `lab-only-not-a-real-secret-ms-analisis` |

`definitions.json` solo contiene el **hash** de cada contraseña
(`password_hash`, algoritmo SHA-256 + salt de 4 bytes documentado en
<https://www.rabbitmq.com/docs/passwords>, ver
`progress/explore_definitions_format.md` §7) — RabbitMQ no ofrece un
mecanismo confirmado de importar contraseñas en texto plano, y aunque lo
ofreciera no se comitearían en claro. El valor en texto plano vive en un
único lugar del lado Rust,
[`src/lib.rs::lab_credentials`](../src/lib.rs), que además documenta esta
misma tabla en sus doc-comments; si se cambia una contraseña aquí hay que
recalcular su hash y actualizar `definitions.json` a la vez (no están
sincronizados automáticamente).

Antes de la feature `topology_definition`, `docker-compose.yml` creaba el
usuario `lab-admin` vía las variables de entorno
`RABBITMQ_DEFAULT_USER`/`RABBITMQ_DEFAULT_PASS`. Desde esta feature,
`definitions.json` es la única fuente de verdad de usuarios/permisos
(incluido `lab-admin`) — mantener ambos mecanismos a la vez duplicaría esa
fuente de verdad sin necesidad; las credenciales de `lab-admin` no
cambiaron.

## Próximas features que tocan este archivo

- **Feature `cancellation_contract`** (id 5): exchange `scan.cancellations`
  y cola `ms-nmap.scan-cancellations`, ampliando permisos de `gateway` y
  `ms-nmap`.
