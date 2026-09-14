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
Gateway ──(ScanRequest)──▶  Broker  ──▶  ms-nmap
                                          │
                                          ▼
                              Broker  ◀── (ScanOutcome)
                                │
                                ▼
                           ms-analisis
```

`ms-usuarios` **no** pasa por el Broker: habla directo con el Gateway. Si en
el futuro se necesita mensajería asíncrona para `ms-usuarios`, es una
decisión de producto que se discute y se documenta como feature nueva — no
se asume aquí.

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

## Capas / directorios

1. **`docker-compose.yml`** (raíz) — RabbitMQ local (`management` +
   AMQPS), con `rabbitmq/definitions.json` cargado al arrancar. Entorno de
   referencia para desarrollo y para los tests de integración.
2. **`rabbitmq/`** — `definitions.json`: vhost, exchanges, colas, bindings,
   colas de mensajes muertos (dead-letter), usuarios y sus permisos. Es la
   fuente de verdad de la topología real.
3. **`contracts/`** — JSON Schema de cada tipo de mensaje (`ScanRequest`,
   `ScanOutcome`) + documentación (`contracts/README.md`) de qué
   exchange/routing-key transporta cada uno y quién publica/consume.
4. **`src/`, `tests/`** — crate Rust de verificación. `tests/` prueba la
   topología y los permisos contra un contenedor real
   (`#[ignore = "requiere Docker"]`, igual convención que `ms-nmap`);
   `src/` solo contiene helpers puros que necesiten esos tests (p. ej.
   validación de un payload contra un JSON Schema) — sin lógica de negocio,
   sin cliente de producción.

No introducir capas adicionales hasta que haya una razón concreta
documentada en `feature_list.json`.

## Contrato de mensajes (referencia — se congela en la feature `message_contract`)

Formato ya implementado y testeado en `ms-nmap` (`src/domain.rs`,
`src/messaging/`). `contracts/` debe documentarlo **exactamente así**, sin
inventar campos nuevos (p. ej. un `schema_version`) sin discutirlo antes con
el usuario:

**`ScanRequest`** (Gateway publica, `ms-nmap` consume):
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

**`ScanOutcome`** (`ms-nmap` publica, `ms-analisis` consume), internamente
tageado por `status`:
```json
{"status": "completed", "correlation_id": "...", "result": { "...ScanResult...": "" }}
```
```json
{"status": "failed", "correlation_id": "...", "reason": "..."}
```

## Manejo de errores

- El crate de verificación define sus propios tipos de error con
  `thiserror` (variantes específicas, no `String` genérico) para la lógica
  que tenga (p. ej. validación de schema).
- Un fallo de un test de integración nunca se "arregla" reemplazándolo por
  un mock — se documenta el bloqueo (p. ej. Docker no disponible) en
  `progress/current.md`.

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
