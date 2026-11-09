# Arquitectura — Qué significa "hacer un buen trabajo"

> Este documento define el estándar de calidad. Los agentes revisores
> evalúan el trabajo contra este archivo. Si no está aquí, no es un requisito.

## Alcance de este repo

Este repo implementa **únicamente el contrato de mensajes y la topología
esperada (SNS + SQS) de la mensajería** de la plataforma. `ms-usuarios`, `ms-nmap` (ya
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
   └──────────────(gateway-scan-outcomes)───────  Broker  ──(ms-analisis-scan-outcomes,
                                                              diferida: solo completed/failed)──▶ ms-analisis
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
  **consume** desenlaces (cola `gateway-scan-outcomes`), no solo los publica
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

- **Tecnología: AWS SNS + SQS** (decisión del usuario, 2026-10-17; reemplaza a
  RabbitMQ y a Amazon MQ, denegado en el Learner Lab). Ver `docs/decisions.md`
  D-13 y el diseño firmado en `odd/tasks/sqs-sns-migration.md`.
- **Alcance: solo contrato + topología esperada, sin librería compartida.**
  Este repo define el formato de los mensajes (`contracts/`) y la topología
  (`src/topology.rs`); `infra` la implementa. **No publica ningún crate para
  que otros servicios lo importen.** Cada servicio implementa su propio cliente.
- **Topología: tópico por flujo, cola por consumidor.** `scan-requests`,
  `scan-cancellations` y `scan-outcomes`; colas `ms-nmap-scan-requests`,
  `ms-nmap-scan-cancellations` y `gateway-scan-outcomes`, cada una con su
  `-dlq`. La routing key viaja como atributo de mensaje `routing_key`; las
  suscripciones usan `RawMessageDelivery=true` y, para el Gateway, una
  `FilterPolicy` por prefijo `scan.outcome.`.
- **Reintentos acotados (RNF-06):** `RedrivePolicy.maxReceiveCount=3` hacia la
  DLQ. Un mensaje malformado se envía explícitamente a la DLQ (decisión Q5,
  responsabilidad del consumidor).
- **Semántica at-least-once, colas estándar.** Los consumidores deben ser
  idempotentes por `correlation_id`; no hay orden garantizado (decisión Q3).
- **Sensibilidad:** el `ScanRequest` lleva una credencial SSH real
  (`ssh_credentials_ref`; ver `docs/security-scope.md`). SSE-SQS en todas las
  colas, retención de 1 h en la cola de requests y 24 h en su DLQ, HTTPS-only
  por política.
- **El contrato de mensajes se copia literal desde `ms-nmap`, no se
  re-deriva.** El JSON Schema debe coincidir con lo que `ms-nmap` ya
  serializa/deserializa.
- **Verificación con `testcontainers` + LocalStack**, sin mocks ni cuentas AWS:
  un crate Rust cuyo único propósito es probar contrato, enrutamiento, DLQ y
  atributos, más una comprobación offline de paridad con `infra`. No produce
  un binario de producción ni una imagen Docker de servicio.
- **Observabilidad:** las métricas de colas (mensajes pendientes, mensajes en
  DLQ; RNF-09/RNF-10) pasan a ser métricas de CloudWatch de SQS, a cargo de
  `infra`; este repo ya no tiene una API de management que consultar.

## Capas / directorios

1. **`contracts/`** — JSON Schema de cada tipo de mensaje (`ScanRequest`,
   `ScanOutcome`, `ScanCancellation`) + documentación (`contracts/README.md`)
   de qué tópico/routing key transporta cada uno y quién publica/consume.
2. **`src/`** — helpers puros: `contracts.rs` (validación de payloads) y
   `topology.rs` (topología esperada). Sin lógica de negocio, sin cliente de
   producción.
3. **`tests/`** — verificación: LocalStack para contrato, enrutamiento, DLQ y
   atributos (`#[ignore = "requiere Docker"]`); `topology_parity.rs` offline
   contra `../infra`.
4. **`odd/tasks/`** — feature docs (migración a SNS/SQS; `amazon-mq-migration`
   queda como historial).

No introducir capas adicionales hasta que haya una razón concreta
documentada en `docs/decisions.md`.

## Contrato de mensajes (referencia — se congela en las features `message_contract` y `cancellation_contract`)

`ScanRequest` y las variantes `completed`/`failed` de `ScanOutcome` ya están
implementadas y testeadas en `ms-nmap` (`src/domain.rs`, `src/messaging/`).
`contracts/` debe documentarlas **exactamente así**, sin inventar campos
nuevos (p. ej. un `schema_version`) sin discutirlo antes con el usuario. La
variante `started` de `ScanOutcome` y `ScanCancellation` son contrato
**acordado pero pendiente de implementar en `ms-nmap`** — este repo define
el canal y el schema, no la publicación/consumo real.

**`ScanRequest`** (tópico `scan-requests`, `routing_key` `scan.request` —
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
el Gateway consume las tres (`scan.outcome.#`, cola `gateway-scan-outcomes`,
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
`started` ya lo publica `ms-nmap` (verificado en el diseño, Q8).

**`ScanCancellation`** (tópico `scan-cancellations`, `routing_key`
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
  el feature doc de `odd/tasks/`.

## Despliegue

Este repo no empaqueta ningún binario ni imagen de servicio. La topología se
despliega desde `infra` (`terraform/messaging.tf`; init local en
`scripts/localstack-init.sh`) y `tests/topology_parity.rs` evita que
`src/topology.rs` y esos archivos diverjan. No se asume ni se documenta aquí
nada específico de cada servicio más allá de que reciben ARN de tópico y URL de
cola por variables de entorno definidas en `infra`.

## Qué NO hacer

- No construir un cliente/crate compartido para que otros servicios lo
  importen sin que el usuario lo pida explícitamente (ver "Decisiones de
  diseño").
- No versionar el esquema del contrato (`schema_version` u otro mecanismo)
  sin discutirlo antes — se deja como nota abierta hasta que haga falta.
- No probar contra una cuenta AWS real de staging/producción: solo
  LocalStack.
- No usar HTTP en claro salvo contra el LocalStack efímero de los tests.
