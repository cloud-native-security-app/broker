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

## Despliegue

Este repo no produce ninguna imagen Docker de servicio ni un binario de
producción (ver "Capas / directorios" en `docs/architecture.md`): lo que se
despliega en un entorno real es la **topología de RabbitMQ**
(`rabbitmq/definitions.json`) sobre una instancia de RabbitMQ ya existente
— la instancia en sí (gestionada, en un clúster propio, o en el proveedor
cloud que se decida) está fuera del alcance de este repo. El *por qué* de
cada decisión de esta sección está justificado en `docs/architecture.md`
§"Despliegue".

### Cargar la topología en una instancia real

`rabbitmq/definitions.json` (vhost, exchanges, colas, bindings, usuarios y
permisos — features `topology_definition`, `tls`, `cancellation_contract`)
se puede aplicar a cualquier instancia de RabbitMQ por, al menos, estas 3
vías, sin asumir cuál se usará mientras no se decida una plataforma de
despliegue concreta:

1. **API de management** (`POST /api/definitions` sobre el vhost
   correspondiente) — ya usada por los tests de este repo y documentada en
   `rabbitmq/README.md` §"Observabilidad" para las consultas de solo
   lectura; para *cargar* definiciones es la misma API HTTP, autenticada
   como un usuario administrador real (nunca `lab-admin`, ver "Qué cambia
   respecto al `docker-compose.yml` local" abajo) contra el host real en
   vez de `localhost:15672`.
2. **`rabbitmqctl import_definitions <archivo>`** — ejecutado en un nodo
   del clúster real (o vía `docker exec`/`kubectl exec` si RabbitMQ corre
   contenedorizado), apuntando al mismo `rabbitmq/definitions.json` de este
   repo.
3. **El mecanismo del proveedor cloud/plataforma elegido** — *a definir
   según el proveedor que se decida* (p. ej., solo como ejemplo
   ilustrativo y no como decisión tomada, un operador de Kubernetes para
   RabbitMQ o un servicio gestionado con su propio import de definiciones).
   Este repo no asume AWS/GCP/Azure ni ningún servicio gestionado concreto
   porque esa decisión de arquitectura no se ha tomado; si se toma, se
   documenta aquí como actualización de esta misma sección.

En los tres casos el archivo fuente sigue siendo
`rabbitmq/definitions.json` de este repo — no se genera un archivo
distinto para producción; lo que cambia son los **valores sensibles** que
contiene (ver el punto siguiente).

### Qué cambia respecto al `docker-compose.yml` local

El `docker-compose.yml` de la raíz es **solo el entorno de referencia
local** (ver `docs/architecture.md` §"Decisiones de diseño ya tomadas").
Para un despliegue real:

- **Certificados TLS reales (no autofirmados)**: ver `rabbitmq/README.md`
  §"Qué cambia para certificados reales de producción" (feature `tls`) —
  las claves de `rabbitmq.conf` (`ssl_options.cacertfile`/`certfile`/
  `keyfile`) no cambian de forma, solo el origen de esos archivos (CA/cert
  reales en vez de los generados por `generate-lab-certs.sh`).
- **Contraseñas de los usuarios de servicio gestionadas fuera del repo**:
  las contraseñas de laboratorio documentadas en `rabbitmq/README.md`
  §"Credenciales de laboratorio" (y su `password_hash` en
  `definitions.json`) solo son válidas para `docker-compose.yml`/
  `testcontainers`. En producción, cada contraseña (de `gateway`,
  `ms-nmap`, `ms-analisis` y del administrador) se genera fuera de este
  repo y se inyecta al `definitions.json` real (o a la llamada
  `import_definitions`/API de arriba) desde una variable de entorno o un
  gestor de secretos (Vault, Secrets Manager/Key Vault del proveedor, un
  Kubernetes Secret, etc.) — nunca hardcodeada ni commiteada en este repo,
  ni siquiera como hash.
- **UI de management no expuesta públicamente**: el puerto 15672 (igual
  que el AMQP en claro 5672, ver `rabbitmq/README.md` §"Puerto 5672 (AMQP
  en claro)") solo se publican directamente en `docker-compose.yml` local.
  En producción, el puerto 15672 se restringe a acceso interno (red
  privada, VPN, túnel SSH/bastion, o un proxy con su propia autenticación)
  — nunca expuesto directamente a Internet.
- **Monitoreo conectado a un sistema de alertas real**: los endpoints de
  la feature `observability` (`/api/healthchecks/node`,
  `/api/queues/<vhost>/<queue>`, ver `rabbitmq/README.md`
  §"Observabilidad") están pensados para que un exporter/scraper (p. ej.
  el propio `rabbitmq_prometheus` ya habilitado en la imagen oficial, o un
  script/servicio que consulte esos endpoints HTTP periódicamente) los
  consuma y alimente el sistema de alertas real de la organización — sin
  asumir aquí un proveedor de monitoreo/alertas concreto.

### Cómo cada servicio apunta a la instancia real

En los tres casos siguientes, las credenciales **nunca viven en este
repo** (ver `docs/security-scope.md`) — lo que sigue es solo cómo se
referencian.

- **`ms-nmap`** ya expone 2 variables de entorno reales para el Broker
  (confirmadas en `nmap-service/README.md`):

  | Variable | Qué debe contener con RabbitMQ como Broker ya decidido |
  |---|---|
  | `MS_NMAP_BROKER_ENDPOINT` | La URI AMQPS completa: `amqps://<host>:5671/security-app` |
  | `MS_NMAP_BROKER_CREDENTIAL` | La contraseña del usuario `ms-nmap` — nunca el valor en claro aquí, inyectada desde el mismo gestor de secretos del punto anterior |

- **`gateway`** y **`ms-analisis`**: no hay, desde este repo, ningún otro
  repo con variables de entorno reales confirmadas, así que se documentan
  en términos genéricos: cada uno necesita el host del Broker real, el
  vhost (`security-app`), su propio usuario de servicio ya definido en
  `rabbitmq/definitions.json` (`gateway`/`ms-analisis` respectivamente), y
  su contraseña — obtenida siempre de fuera de este repo (variable de
  entorno o gestor de secretos del despliegue de cada servicio), nunca de
  un valor documentado aquí.
