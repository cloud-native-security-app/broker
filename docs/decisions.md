# Decisiones de diseño

Registro breve (estilo ADR) de las decisiones tomadas durante la construcción
del Broker: decisión, contexto y consecuencia. El estado actual de la
topología y del contrato vive en `rabbitmq/`, `contracts/` y
`docs/architecture.md`; este archivo explica el *por qué*.

## D-01 RabbitMQ como tecnología del Broker

- **Decisión:** RabbitMQ (reemplaza menciones anteriores de SQS/NATS/Kafka).
- **Contexto:** el sistema necesita enrutamiento por topic, reintentos acotados
  y dead-lettering por mensaje.
- **Consecuencia:** el repo define topología y contrato; no publica ninguna
  librería compartida. Cada servicio implementa su propio cliente AMQP.

## D-02 Imagen oficial con `load_definitions` en `rabbitmq.conf`

- **Decisión:** montar `definitions.json` y declarar
  `load_definitions = /etc/rabbitmq/definitions.json` (sin prefijo
  `management.`).
- **Contexto:** `RABBITMQ_LOAD_DEFINITIONS` pertenece a la imagen de Bitnami,
  no a la oficial, y las variables `RABBITMQ_*` están deprecadas desde 3.9. La
  clave con prefijo `management.` no carga policies de forma fiable en el
  primer arranque.
- **Consecuencia:** `definitions.json` es la única fuente de verdad de
  vhost, usuarios, permisos, exchanges, colas y bindings. Alternativas para
  instancias existentes: `POST /api/definitions` o `rabbitmqctl
  import_definitions`.

## D-03 Tipo de cola y reintentos: quorum + `x-delivery-limit`

- **Decisión:** colas principales quorum con `x-delivery-limit: 3` y
  dead-letter exchange propio; una `.dlq` por cola principal.
- **Contexto:** alternativa evaluada: patrón clásico cola-retry con
  `x-message-ttl` + DLX. El límite nativo evita cola intermedia y lógica de
  conteo en el consumidor; RabbitMQ dead-letterea con `reason: delivery_limit`.
  El tipo de cola va en `arguments["x-queue-type"]`; RabbitMQ 4.x mantiene
  `classic` por defecto.
- **Consecuencia:** hay que rechazar con `basic.reject`, no `basic.nack`, para
  que las redeliveries cuenten (verificado empíricamente). N=3 significa 4
  entregas totales. El reintento es inmediato, sin backoff.

## D-04 Mínimo privilegio por servicio

- **Decisión:** un usuario por servicio (`gateway`, `ms-nmap`, `ms-analisis`)
  con permisos `configure/write/read` por regex exacta, más `lab-admin` solo
  para laboratorio y monitoreo. Los usuarios de servicio no tienen
  `configure` (`^$`).
- **Contexto:** el `ScanRequest` transporta una credencial SSH real.
- **Consecuencia:** cada ampliación de canal (p. ej. cancelaciones) amplía solo
  el regex necesario; `ms-analisis` no se toca. Contraseñas en
  `definitions.json` como hash SHA-256; en un despliegue real se inyectan
  desde un gestor de secretos.

## D-05 TLS obligatorio; AMQP en claro solo para desarrollo

- **Decisión:** listener AMQPS en 5671 con CA y certificado de laboratorio
  generados por `rabbitmq/generate-lab-certs.sh` (fuera de git). El puerto 5672
  se mantiene abierto solo en el compose local y está marcado como
  solo-desarrollo (decisión del usuario).
- **Contexto:** el mensaje lleva una credencial SSH real.
- **Consecuencia:** los certificados reales cambian solo el origen de los
  archivos, no las claves de `rabbitmq.conf`. El nodo no arranca sin los `.pem`
  montados, por lo que los tests también los montan.

## D-06 Cliente de pruebas: `lapin` + `rustls` con proveedor fijado

- **Decisión:** `lapin` con su backend `rustls` por defecto; la CA de
  laboratorio se confía en runtime vía `OwnedTLSConfig.cert_chain`. En
  dev-dependencies se instala explícitamente el `CryptoProvider` `aws_lc_rs`
  (`rustls = "0.23"`).
- **Contexto:** el árbol trae dos proveedores (`ring` y `aws_lc_rs`); sin
  fijar uno, la conexión AMQPS quedaba colgada en vez de fallar.
- **Consecuencia:** el test negativo de TLS usa un `TcpStream` crudo con el
  header AMQP en claro contra 5671, que es más determinista que `amqp://`.

## D-07 Tests de integración reales con `testcontainers`

- **Decisión:** `testcontainers` + `testcontainers-modules` (feature
  `rabbitmq`) con puertos efímeros; `lapin` para AMQP y `reqwest` async contra
  la API de management. Tests marcados `#[ignore = "requiere Docker"]`.
- **Contexto:** nunca se prueba contra un broker real de staging/producción y
  nunca con mocks. Un permiso denegado se detecta como `ACCESSREFUSED`
  (soft error, cierra el canal). La topología se verifica con `passive: true`
  y los bindings exactos vía API de management.
- **Consecuencia:** las estadísticas de management tienen retraso (hasta ~2 s),
  por lo que los tests sondean en vez de asumir inmediatez.

## D-08 Contrato de mensajes como JSON Schema congelado

- **Decisión:** JSON Schema draft 2020-12 en `contracts/` para `ScanRequest`,
  `ScanOutcome` (variantes `started`/`completed`/`failed` discriminadas por
  `status`) y `ScanCancellation`, con `additionalProperties: false`. El shape
  se copió del código real de `ms-nmap`, no se re-derivó.
- **Contexto:** `ip` se valida con `pattern` porque el crate `jsonschema` no
  valida `format` por defecto. No existe `schema_version`; los cambios deben
  ser aditivos.
- **Consecuencia:** `started` y la honra de la cancelación son contrato
  acordado pero pendiente de implementar en `nmap-service`. Los helpers de
  `src/contracts.rs` compilan los schemas una sola vez por proceso;
  `ScanCancellation` nunca lleva credenciales.

## D-09 Observabilidad vía API de management

- **Decisión:** health check del nodo (`/api/healthchecks/node`) y métricas de
  colas (`/api/queues/<vhost>/<queue>`) por la API de management, siempre con
  `lab-admin`. No se publica el puerto 15692 de `rabbitmq_prometheus`
  (habilitado por defecto en la imagen).
- **Contexto:** no hay Prometheus/Grafana en el laboratorio y la API cubre
  RNF-09/RNF-10.
- **Consecuencia:** exponer métricas Prometheus queda como decisión futura
  explícita.

## D-10 Despliegue: topología desacoplada de la plataforma

- **Decisión:** el repo no asume proveedor; documenta tres vías de carga de
  `definitions.json` (API de management, `rabbitmqctl`, mecanismo del
  proveedor) y qué cambia respecto al compose local (certificados reales,
  secretos externos, management no expuesto, alertas reales).
- **Contexto:** `ms-nmap` ya expone `MS_NMAP_BROKER_ENDPOINT` y
  `MS_NMAP_BROKER_CREDENTIAL`; para el resto de servicios solo se documenta de
  forma genérica.
- **Consecuencia:** la migración a Amazon MQ está planificada en
  `odd/tasks/amazon-mq-migration.md`.

## D-11 Verificación única y reproducible

- **Decisión:** una secuencia fija (fmt, clippy `-D warnings`, tests
  unitarios, tests de integración con Docker, doc, `docker compose config`)
  expuesta como `make verify`.
- **Contexto:** los tests de integración que fallan por falta de Docker se
  reportan como bloqueo; no se reemplazan por mocks.
- **Consecuencia:** ver `docs/verification.md`.

## D-12 Dependencias podadas

- **Decisión:** se eliminó `futures` de `Cargo.toml` por no usarse.
- **Consecuencia:** el crate solo declara dependencias con uso real.
