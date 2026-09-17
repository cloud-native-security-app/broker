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
| `scan.cancellations` | `topic` | `scan.cancellations.dlx` (`direct`) |

| Cola | Bindeada a | Routing key(s) | Dead-letter queue |
|---|---|---|---|
| `ms-nmap.scan-requests` | `scan.requests` | `scan.request` | `ms-nmap.scan-requests.dlq` |
| `ms-analisis.scan-outcomes` | `scan.outcomes` | `scan.outcome.completed`, `scan.outcome.failed` (**nunca** `scan.outcome.started`) | `ms-analisis.scan-outcomes.dlq` |
| `gateway.scan-outcomes` | `scan.outcomes` | `scan.outcome.#` (started, completed, failed) | `gateway.scan-outcomes.dlq` |
| `ms-nmap.scan-cancellations` | `scan.cancellations` | `scan.cancellation` | `ms-nmap.scan-cancellations.dlq` |

`ms-nmap.scan-cancellations` (feature `cancellation_contract`, RF-14) es el
canal por el que el Gateway pide cancelar un escaneo en curso; ver
`contracts/README.md` §`ScanCancellation` — `ms-nmap` consumir y honrar la
cancelación es trabajo pendiente en el repo `nmap-service`, no implementado
todavía.

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
`gateway.scan-outcomes`, `ms-nmap.scan-cancellations`) es una **cola
quorum** (`"x-queue-type": "quorum"`)
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
| `gateway` | `^$` | `^scan\.(requests\|cancellations)$` | `^gateway\.scan-outcomes$` |
| `ms-nmap` | `^$` | `^scan\.outcomes$` | `^ms-nmap\.(scan-requests\|scan-cancellations)$` |
| `ms-analisis` | `^$` | `^$` | `^ms-analisis\.scan-outcomes$` |

Ningún usuario de servicio puede leer la cola de otro servicio ni escribir
fuera de su exchange: `gateway` no puede leer `ms-nmap.scan-requests`,
`ms-nmap.scan-cancellations` ni `ms-analisis.scan-outcomes`; `ms-nmap` no
puede escribir en `scan.requests` ni leer las colas de
`gateway`/`ms-analisis`; `ms-analisis` no puede escribir nada ni tiene
ningún acceso a `scan.cancellations`/`ms-nmap.scan-cancellations` (feature
`cancellation_contract`). `configure: "^$"` en los tres usuarios de
servicio: ninguno declara/borra topología — eso solo lo hace
`definitions.json` (vía el usuario `lab-admin`).

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

## TLS (AMQPS, feature `tls`)

Toda conexión de producción a este Broker debe usar AMQPS (TLS), nunca
AMQP en claro — el `ScanRequest` que atraviesa `scan.requests` lleva una
credencial SSH real en `ssh_credentials_ref` (ver `docs/security-scope.md`).

### Setup (paso obligatorio antes de `docker compose up`)

Los certificados de laboratorio **no se commitean** al repo (`.gitignore`
cubre `*.pem`/`*.key` desde la feature `scaffolding`): son regenerables y
la clave privada, aunque sea de laboratorio, no se versiona por higiene.
Antes de levantar el stack por primera vez (o si `rabbitmq/tls/` no
existe):

```bash
./rabbitmq/generate-lab-certs.sh
```

Esto genera, en `rabbitmq/tls/` (usando `openssl`, sin dependencias
externas):

| Archivo | Contenido |
|---|---|
| `ca_certificate.pem` | CA de laboratorio autofirmada (pública) |
| `ca_key.pem` | Clave privada de la CA de laboratorio |
| `server_certificate.pem` | Certificado de servidor, firmado por la CA de arriba, con SAN `DNS:rabbitmq,DNS:localhost,IP:127.0.0.1` |
| `server_key.pem` | Clave privada del servidor |

**NO usar en producción**: el `CN`/`O` del subject (`O=security-app-lab`,
`CN=security-app-lab-CA` / `CN=rabbitmq`) deja explícito que es material de
laboratorio, y la clave privada de la CA queda en texto plano en disco —
aceptable solo para un contenedor RabbitMQ desechable de desarrollo/test.

### Configuración del listener

`rabbitmq/rabbitmq.conf` habilita el listener AMQPS en el puerto 5671:

```ini
listeners.ssl.default = 5671

ssl_options.cacertfile = /etc/rabbitmq/tls/ca_certificate.pem
ssl_options.certfile   = /etc/rabbitmq/tls/server_certificate.pem
ssl_options.keyfile    = /etc/rabbitmq/tls/server_key.pem
```

Sin `ssl_options.verify`/`ssl_options.fail_if_no_peer_cert`: RabbitMQ usa
por defecto `verify_none` (TLS de transporte servidor, sin exigir
certificado de cliente/mTLS) — suficiente para el requisito de
`docs/security-scope.md` ("toda conexión debe usar AMQPS"), que no exige
autenticación mutua por certificado.

`docker-compose.yml` monta `rabbitmq/tls/` completo en
`/etc/rabbitmq/tls:ro`, igual patrón que `definitions.json`/`rabbitmq.conf`.
Los tests de integración de `tests/` (vía `testcontainers`) montan los
mismos archivos en sus propios contenedores efímeros.

### Puerto 5672 (AMQP en claro) — solo desarrollo/depuración

`docker-compose.yml` sigue publicando el puerto 5672 sin TLS, marcado
explícitamente como solo-desarrollo/depuración (p. ej. para
`rabbitmqctl`/herramientas locales que no manejan TLS). **Ningún servicio
real** (`gateway`, `ms-nmap`, `ms-analisis`) debe conectarse por ahí: la
única forma válida de operar contra este Broker fuera del propio
contenedor es AMQPS (5671). No se usa `listeners.tcp = none` para
deshabilitarlo del todo porque, en este entorno de laboratorio, prevalece
tener una vía simple de depuración manual — la mitigación real es que
ningún servicio tenga nunca una URI `amqp://` (sin "s") apuntando a este
Broker documentada como válida.

### Qué cambia para certificados reales de producción

Las claves de `rabbitmq.conf` (`listeners.ssl.default`,
`ssl_options.cacertfile`/`certfile`/`keyfile`) **no cambian de forma ni de
nombre** — RabbitMQ configura igual un certificado autofirmado que uno
emitido por una CA real. Lo único que cambia es **el origen de los
archivos**:

- `ssl_options.cacertfile` → el bundle de la CA pública/corporativa real
  que emitió el certificado de servidor, en vez de
  `rabbitmq/tls/ca_certificate.pem` generado por
  `generate-lab-certs.sh`.
- `ssl_options.certfile`/`ssl_options.keyfile` → el certificado y la clave
  privada emitidos por esa CA real (p. ej. vía ACME/Let's Encrypt, una CA
  corporativa, o el gestor de certificados/secretos del proveedor cloud),
  en vez de los generados por el script de laboratorio.
- La clave privada real **nunca** se commitea a este repo: en un
  despliegue real, `ssl_options.keyfile` apuntaría a una ruta montada desde
  un secret gestionado por la plataforma (Kubernetes Secret, Docker Swarm
  secret, Key Vault/Secrets Manager del proveedor cloud, etc.) — mismo
  principio que ya aplica este repo a las contraseñas de los usuarios de
  RabbitMQ (ver "Credenciales de laboratorio" arriba).
- Si en algún momento se decide exigir mTLS, se añadirían
  `ssl_options.verify = verify_peer` y `ssl_options.fail_if_no_peer_cert =
  true` juntas (una sin la otra no basta) — fuera del alcance de esta
  feature.
- Renovación/rotación de certificados reales es responsabilidad de quien
  opere el despliegue (cert-manager, ACME, rotación del proveedor cloud) —
  fuera del alcance de este repo de infraestructura de Broker.

## Observabilidad (health checks y métricas de colas, feature `observability`)

RNF-09 (estado del Broker consultable) y RNF-10 (métricas de cola
consultables) se cubren enteramente con la API HTTP de management ya
expuesta desde la feature `scaffolding` (puerto 15672, mismo usuario
`lab-admin` que el resto de consultas de solo-lectura de este repo — ver
"Usuarios y permisos de mínimo privilegio" y "Credenciales de
laboratorio" arriba: nunca un usuario de servicio, que ni siquiera tiene
el tag `administrator` para autenticarse contra la API). No se despliega
Prometheus/Grafana en este repo de laboratorio — ver "Decisión sobre
`rabbitmq_prometheus`" abajo.

### Endpoints usados

| Endpoint | Para qué | Ejemplo de respuesta (verificado contra el contenedor real, `rabbitmq:4.3.5-management`) |
|---|---|---|
| `GET /api/healthchecks/node` | Estado del nodo (RNF-09) | `{"status":"ok"}` |
| `GET /api/queues/<vhost>/<queue>` | Mensajes listos (`messages_ready`) y consumidores activos (`consumers`) por cola (RNF-10) | `{"messages_ready": 3, "consumers": 0, ...}` |

`<vhost>` es siempre `security-app` (ver "Topología" arriba). Se consulta
`/api/queues/security-app/<queue>` para cada una de las 8 colas
declaradas en `rabbitmq/definitions.json` (features `topology_definition`
y `cancellation_contract`):

- `ms-nmap.scan-requests` / `ms-nmap.scan-requests.dlq`
- `ms-analisis.scan-outcomes` / `ms-analisis.scan-outcomes.dlq`
- `gateway.scan-outcomes` / `gateway.scan-outcomes.dlq`
- `ms-nmap.scan-cancellations` / `ms-nmap.scan-cancellations.dlq`

Para las 4 colas `.dlq`, el campo relevante es el mismo `messages_ready`:
un valor mayor que 0 significa que hay mensajes muertos pendientes de
investigar/reprocesar manualmente — ver "Qué NO expone esta feature"
abajo sobre por qué no se inspecciona su cuerpo desde aquí.

### Autenticación

Igual que `tests/topology_exists.rs`: HTTP Basic con el usuario
`lab-admin` (ver "Credenciales de laboratorio" arriba). Ningún usuario de
servicio (`gateway`, `ms-nmap`, `ms-analisis`) tiene el tag
`administrator`, así que ni siquiera puede autenticarse contra la API de
management (`docs/security-scope.md` §"Usuarios y permisos de mínimo
privilegio") — el monitoreo de este repo se hace siempre como
administrador, nunca como un usuario de servicio productor/consumidor.

### Qué NO expone esta feature

Los dos endpoints de arriba devuelven **conteos y metadatos** (estado del
nodo, profundidad de cola, número de consumidores) — nunca el cuerpo de
un mensaje. No hay riesgo adicional de fuga del `ssh_credentials_ref` de
`ScanRequest` por esta vía (ver `docs/security-scope.md` §"Cobertura de
las features añadidas en la ronda 2"). La UI/API de management sigue sin
exponerse fuera de `docker-compose.yml` local (mismo puerto 15672 ya
documentado ahí, solo para desarrollo).

### Decisión sobre el plugin `rabbitmq_prometheus`

**No se publica ningún puerto adicional para él en `docker-compose.yml`.**
Verificado contra el contenedor real (`rabbitmq:4.3.5-management`, vía
`rabbitmq-plugins list -e`): el plugin `rabbitmq_prometheus` viene
**habilitado por defecto** en la imagen oficial desde RabbitMQ 3.8+
(junto con `rabbitmq_management`), sirviendo métricas en el puerto
interno 15692 del contenedor — no es algo que este repo tenga que
habilitar explícitamente en `rabbitmq.conf`/`definitions.json`. La
decisión que sí toma este repo es **no publicar `15692:15692` en
`docker-compose.yml`** (a diferencia de 5672/5671/15672, que sí se
publican): no hay ningún Prometheus/Grafana desplegado en este repo de
laboratorio que vaya a scrapearlo, y esta feature ya cubre RNF-09/RNF-10
por completo con la API HTTP de management (`/api/healthchecks/node`,
`/api/queues/...`), que además es la misma vía que ya usan los tests de
`topology_definition` (`tests/topology_exists.rs`). Publicar un puerto
adicional sin ningún consumidor real solo ampliaría la superficie
expuesta del contenedor de desarrollo sin ningún beneficio de
observabilidad adicional ahora mismo. Si en el futuro se decide desplegar
Prometheus/Grafana como parte de la plataforma, es una feature nueva a
discutir explícitamente (qué se publica, con qué autenticación/red, y su
propio análisis de `docs/security-scope.md` si expone algo nuevo) — no se
asume aquí.

### Tests (`tests/observability.rs`)

- `node_healthcheck_reports_ok`: el healthcheck del nodo responde
  `200 OK` con `{"status":"ok"}` contra el contenedor con la topología
  cargada.
- `publishing_n_messages_reports_matching_queue_depth`: publica 3
  mensajes en `scan.requests`/`scan.request` sin consumirlos y verifica
  que `/api/queues/security-app/ms-nmap.scan-requests` reporta
  `messages_ready: 3` y `consumers: 0` (con reintentos cortos: las
  estadísticas de la API de management no son instantáneas — se tardó
  hasta un par de segundos en reflejar publicaciones recientes al
  verificarlo empíricamente contra el contenedor real, así que el test
  sondea en vez de asumir una única lectura inmediata).
- `exhausted_message_visible_in_dlq_via_management_api`: reutiliza el
  mecanismo de `tests/retry_delivery_limit.rs` (publicar + `basic.reject`
  ×3 hasta agotar `x-delivery-limit`) y verifica, vía
  `/api/queues/security-app/ms-nmap.scan-requests.dlq`, que
  `messages_ready` refleja el mensaje muerto — sin leerlo por AMQP en
  este test, porque lo que se está probando es que la propia API de
  management lo refleja.

## Próximas features que tocan este archivo

- **Feature `cancellation_contract`** (id 5): exchange `scan.cancellations`
  y cola `ms-nmap.scan-cancellations`, ampliando permisos de `gateway` y
  `ms-nmap`.
