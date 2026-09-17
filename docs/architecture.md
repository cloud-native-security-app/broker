# Arquitectura — Qué significa "hacer un buen trabajo"

> Este documento define el estándar de calidad. Los agentes revisores
> evalúan el trabajo contra este archivo. Si no está aquí, no es un requisito.

## Alcance de este repo

Este repo implementa **únicamente la infraestructura y el contrato de
mensajes del Broker** de la plataforma. `ms-usuarios`, `ms-nmap` (ya
implementado, otro repo), `ms-analisis` y el Gateway son otros
servicios/otros repos — no se implementan aquí, y este repo **no** conoce su
lógica de negocio.

Del diagrama de arquitectura del sistema (`Diagrama_Arquitectura_Solucion_
Cloud_Native_I-Página-2`):

```
Gateway ──(ScanRequest)──────────▶  Broker  ──▶  ms-nmap
   ▲                                                │
   │                                                │ (ScanOutcome: started/completed/failed)
   │                                                ▼
   └──────────────(gateway.scan-outcomes)───────  Broker  ──(ms-analisis.scan-outcomes:
                                                      │       solo completed/failed)──▶ ms-analisis
Gateway ──(ScanCancellation)─────▶  Broker  ──▶  ms-nmap
```

`ms-usuarios` **no** pasa por el Broker: habla directo con el Gateway. Si en
el futuro se necesita mensajería asíncrona para `ms-usuarios`, es una
decisión de producto que se discute y se documenta como feature nueva — no
se asume aquí.

Ampliación 2026-09-14 (ronda 2, tras leer `documento_requerimientos.docx` y
`Plan de pruebas Cloud Native_I.docx`): el diagrama original solo tenía
`ScanRequest`/`ScanOutcome` en un sentido. Los requisitos oficiales del
proyecto exigen además:
- **RF-08** (notificación en tiempo real al frontend) → el Gateway también
  **consume** desenlaces (cola `gateway.scan-outcomes`), no solo los publica
  el lado de `ms-nmap` hacia `ms-analisis`.
- **RF-07** (estado PENDIENTE/EN_PROGRESO/COMPLETADO/FALLIDO) → `ScanOutcome`
  gana una tercera variante `started`, que solo le interesa al Gateway (para
  reflejar EN_PROGRESO), no a `ms-analisis`.
- **RF-14** (cancelar un escaneo activo) → un mensaje nuevo,
  `ScanCancellation`, que el Gateway publica y `ms-nmap` consume.

Ver "Contrato de mensajes" abajo para el detalle. **Importante:** que
`ms-nmap` realmente publique `started` y consuma/honre la cancelación es
trabajo pendiente en el repo `nmap-service` — no se implementa aquí, solo se
define el canal y el contrato.

## Decisiones de diseño ya tomadas

- **Tecnología: RabbitMQ.** Decisión del usuario (2026-09-14), reemplaza
  cualquier mención anterior de SQS/NATS/Kafka en diagramas viejos del
  proyecto.
- **Alcance: solo infraestructura + contrato, sin librería compartida.**
  Este repo define y verifica la topología de RabbitMQ (`rabbitmq/`) y el
  formato de los mensajes (`contracts/`). **No publica ningún crate Rust
  para que otros servicios lo importen.** Cada servicio implementa su
  propio cliente RabbitMQ (p. ej. con `lapin`) contra el contrato
  documentado aquí. Si en el futuro se decide construir un cliente
  compartido, es una feature nueva a discutir explícitamente — no se asume.
- **`docker-compose.yml` es el entorno de referencia local**, no un
  despliegue de producción. Usa una imagen `rabbitmq:<tag>-management`
  pineada por tag (y por digest cuando sea posible) para tener la UI de
  gestión disponible en desarrollo.
- **La topología se declara de forma imperativa** en
  `rabbitmq/definitions.json` (el formato que RabbitMQ carga vía
  `management.load_definitions` o al arrancar con
  `RABBITMQ_LOAD_DEFINITIONS`) — no se crea a mano ni por scripts
  imperativos sueltos, para que quede versionada y sea reproducible.
- **Usuarios de mínimo privilegio, uno por servicio productor/consumidor**
  (`gateway`, `ms-nmap`, `ms-analisis`), cada uno con permisos (`configure`/
  `write`/`read`) acotados por regex a los exchanges/colas que le
  corresponden. Ningún servicio tiene permisos sobre la infraestructura de
  otro.
- **TLS (AMQPS) obligatorio.** El `ScanRequest` que atraviesa este Broker
  lleva una credencial SSH real en `ssh_credentials_ref` (así está
  implementado en `ms-nmap`) — ver `docs/security-scope.md`. AMQP en claro
  no es una opción de producción; en el `docker-compose.yml` local se
  documenta explícitamente si se deja disponible solo para depuración.
- **El contrato de mensajes se copia literal desde `ms-nmap`, no se
  re-deriva.** `ms-nmap` ya implementa y testea la forma exacta de
  `ScanRequest` y `ScanOutcome` (ver "Contrato de mensajes" abajo); este
  repo es la fuente de verdad documentada de ese contrato para el resto de
  la plataforma, pero el JSON Schema debe coincidir byte a byte con lo que
  `ms-nmap` ya serializa/deserializa.
- **Verificación con `testcontainers` + `lapin`**, igual rigor que
  `ms-nmap`: un crate Rust cuyo único propósito es probar que la topología,
  los permisos y el contrato son correctos contra un `rabbitmq:management`
  real — **nunca** contra un broker de producción, **nunca** con mocks.
  Este crate **no** produce un binario de producción ni una imagen Docker
  de servicio: es solo arnés de verificación.
- **Reintentos limitados antes de dead-letter (RNF-06 y el plan de
  pruebas del proyecto).** Un mensaje que falla su procesamiento se
  reintenta un número acotado de veces (no una vez con TTL directo a DLQ)
  antes de caer en su cola de mensajes fallidos — ver la feature
  `topology_definition`.
- **Observabilidad del Broker es un requisito explícito (RNF-09, RNF-10),
  no un nice-to-have.** Health checks del nodo y métricas de cola
  (mensajes pendientes, consumidores activos, mensajes en DLQ) deben ser
  consultables vía la API de management — ver la feature `observability`
  y su documentación detallada (endpoints exactos, las 8 colas cubiertas,
  y la decisión sobre el plugin `rabbitmq_prometheus`) en
  `rabbitmq/README.md` §"Observabilidad".

## Capas / directorios

1. **`docker-compose.yml`** (raíz) — RabbitMQ local (`management` +
   AMQPS), con `rabbitmq/definitions.json` cargado al arrancar. Entorno de
   referencia para desarrollo y para los tests de integración.
2. **`rabbitmq/`** — `definitions.json`: vhost, exchanges, colas, bindings,
   colas de mensajes muertos (dead-letter), usuarios y sus permisos. Es la
   fuente de verdad de la topología real.
3. **`contracts/`** — JSON Schema de cada tipo de mensaje (`ScanRequest`,
   `ScanOutcome`, `ScanCancellation`) + documentación (`contracts/README.md`)
   de qué exchange/routing-key transporta cada uno y quién publica/consume.
4. **`src/`, `tests/`** — crate Rust de verificación. `tests/` prueba la
   topología y los permisos contra un contenedor real
   (`#[ignore = "requiere Docker"]`, igual convención que `ms-nmap`);
   `src/` solo contiene helpers puros que necesiten esos tests (p. ej.
   validación de un payload contra un JSON Schema) — sin lógica de negocio,
   sin cliente de producción.

No introducir capas adicionales hasta que haya una razón concreta
documentada en `feature_list.json`.

## Contrato de mensajes (referencia — se congela en las features `message_contract` y `cancellation_contract`)

`ScanRequest` y las variantes `completed`/`failed` de `ScanOutcome` ya están
implementadas y testeadas en `ms-nmap` (`src/domain.rs`, `src/messaging/`).
`contracts/` debe documentarlas **exactamente así**, sin inventar campos
nuevos (p. ej. un `schema_version`) sin discutirlo antes con el usuario. La
variante `started` de `ScanOutcome` y `ScanCancellation` son contrato
**acordado pero pendiente de implementar en `ms-nmap`** — este repo define
el canal y el schema, no la publicación/consumo real.

**`ScanRequest`** (exchange `scan.requests`, routing key `scan.request` —
Gateway publica, `ms-nmap` consume):
```json
{
  "correlation_id": "req-2026-0042",
  "ip": "10.20.30.40",
  "network_user": "scanner",
  "ssh_credentials_ref": "el-secreto-ssh",
  "has_sudo": false,
  "requested_by": "analyst@example.test"
}
```
`ssh_credentials_ref` es la credencial SSH **real**, no una referencia
opaca — ver `docs/security-scope.md`.

**`ScanOutcome`** (exchange `scan.outcomes`, internamente tageado por
`status`) — `ms-nmap` publica; `ms-analisis` consume solo `completed`/
`failed` (routing keys `scan.outcome.completed`/`scan.outcome.failed`);
el Gateway consume las tres (`scan.outcome.#`, cola `gateway.scan-outcomes`,
para reflejar PENDIENTE/EN_PROGRESO/COMPLETADO/FALLIDO en el frontend, RF-07/RF-08):
```json
{"status": "started", "correlation_id": "..."}
```
```json
{"status": "completed", "correlation_id": "...", "result": { "...ScanResult...": "" }}
```
```json
{"status": "failed", "correlation_id": "...", "reason": "..."}
```
`started` es la única variante que `ms-nmap` **todavía no publica** — ver
la nota de "trabajo pendiente" arriba.

**`ScanCancellation`** (exchange `scan.cancellations`, routing key
`scan.cancellation` — Gateway publica, `ms-nmap` consume; consumir y
honrar la cancelación es trabajo pendiente en `ms-nmap`, RF-14):
```json
{"correlation_id": "req-2026-0042", "requested_by": "analyst@example.test"}
```

## Manejo de errores

- El crate de verificación define sus propios tipos de error con
  `thiserror` (variantes específicas, no `String` genérico) para la lógica
  que tenga (p. ej. validación de schema).
- Un fallo de un test de integración nunca se "arregla" reemplazándolo por
  un mock — se documenta el bloqueo (p. ej. Docker no disponible) en
  `progress/current.md`.

## Despliegue (feature `deployment_docs`)

> Detalle práctico (las 3 vías de carga, la tabla de variables, referencias
> exactas) vive en `README.md` §"Despliegue" — esta sección explica el
> *por qué* de cada decisión, análogo a la sección "Despliegue" de
> `docs/architecture.md` en `ms-nmap`, no la duplica.

Este repo no empaqueta ningún binario ni imagen de servicio (ver "Capas /
directorios" arriba): el crate Rust es solo arnés de verificación, sin
`src/main.rs`. Lo único "desplegable" que produce este repo es
declarativo: `rabbitmq/definitions.json` — el mismo archivo que
`docker-compose.yml` carga en local es el que se aplica, sin reescribirlo,
a una instancia real.

### Por qué no se asume un proveedor cloud

La feature `deployment_docs` exige explícitamente no asumir un proveedor
si no se ha decidido — mismo principio que "Decisiones de diseño ya
tomadas" arriba: no se inventa infraestructura sin que el usuario la
pida. Nombrar aquí un servicio gestionado concreto (AWS MQ, GCP, Azure
Service Bus, etc.) congelaría una decisión de arquitectura que no le
corresponde tomar a este repo por su cuenta. Si en el futuro se decide un
proveedor, se documenta como actualización de `README.md` §"Despliegue",
no como una feature nueva que intente adivinar la decisión.

### Por qué el `docker-compose.yml` local no es el despliegue real

`docker-compose.yml` es, desde `scaffolding`, el entorno de referencia
para desarrollo y tests, nunca un despliegue de producción. Los 4 puntos
que cambian para producción (certificados TLS reales, contraseñas de
servicio fuera del repo, UI de management no pública, monitoreo
conectado a un sistema de alertas real) ya están detallados con su propia
justificación en `rabbitmq/README.md` (secciones "TLS (AMQPS)",
"Credenciales de laboratorio" y "Observabilidad") y resumidos en
`README.md` §"Despliegue" — no se repiten aquí byte a byte. Ninguno de
los 4 es opcional: los tres primeros mitigan directamente la fuga de la
credencial SSH real que transporta `ScanRequest` (ver
`docs/security-scope.md`); el cuarto (monitoreo → alertas) es lo que
convierte RNF-09/RNF-10 (ya cubiertos por la API de management, feature
`observability`) en observabilidad *operativa* de verdad — sin un sistema
de alertas consumiendo esos endpoints, que respondan bien no avisa a
nadie si el nodo cae o una `.dlq` empieza a crecer.

### Por qué cada servicio se documenta con distinto nivel de certeza

`ms-nmap` ya tiene, en su propio repo, dos variables de entorno reales
para el Broker (`MS_NMAP_BROKER_ENDPOINT`, `MS_NMAP_BROKER_CREDENTIAL`,
confirmadas en `nmap-service/README.md`) — este repo solo documenta qué
debería contener cada una una vez que RabbitMQ es la tecnología decidida
(la URI AMQPS y la contraseña del usuario `ms-nmap`), sin inventar
variables nuevas que `ms-nmap` no exponga ya. `gateway` y `ms-analisis` no
tienen, desde aquí, ningún repo visible con variables reales confirmadas,
así que se documentan en términos genéricos (host, vhost, usuario ya
definido en `rabbitmq/definitions.json`, credencial fuera del repo) en vez
de inventar nombres de variable de entorno concretos que no se puedan
verificar contra código real.

### Trabajo futuro potencial (detectado aquí, NO implementado)

- Un test de integración que ejercite explícitamente
  `rabbitmqctl import_definitions` como mecanismo de carga (los tests
  actuales de `topology_definition` verifican la topología resultante,
  pero vía montaje de archivo + arranque del nodo, no vía el comando
  `import_definitions` en un nodo ya corriendo) — quedaría como una
  feature nueva a decidir explícitamente con el usuario, no se agrega a
  `feature_list.json` en esta sesión.
- Reemplazar el placeholder genérico del proveedor cloud por el mecanismo
  real una vez se decida una plataforma de despliegue concreta.

## Qué NO hacer

- No construir un cliente/crate compartido para que otros servicios lo
  importen sin que el usuario lo pida explícitamente (ver "Decisiones de
  diseño").
- No versionar el esquema del contrato (`schema_version` u otro mecanismo)
  sin discutirlo antes — se deja como nota abierta hasta que haga falta.
- No probar la topología contra una instancia de RabbitMQ real de
  staging/producción.
- No dejar AMQP en claro como opción por defecto en ningún ejemplo o
  `docker-compose.yml`.
