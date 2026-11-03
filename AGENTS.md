# Reglas de revisión de código — broker

> Reglas que usa el revisor automático (Gentleman Guardian Angel) sobre cada
> commit. Un diff que viole una regla marcada como **bloqueante** se rechaza.

## Contexto

`broker` define la infraestructura de mensajería de la plataforma: la
topología de RabbitMQ (`rabbitmq/definitions.json`), su configuración TLS
(`rabbitmq/rabbitmq.conf`), los contratos de mensajes en JSON Schema
(`contracts/`) y un crate Rust (`src/`, `tests/`) que verifica todo eso
contra un RabbitMQ real. No tiene binario de producción. Por
`scan.requests` viajan **credenciales SSH reales**
(`ScanRequest.ssh_credentials_ref`); ver `docs/security-scope.md`.

## Seguridad (bloqueante)

- Nunca se commitean credenciales, certificados ni claves privadas reales.
  Las contraseñas de `definitions.json`, los payloads de ejemplo y los
  certificados de `rabbitmq/tls/` son de laboratorio y deben verse
  ficticios a simple vista (p. ej. `lab-only-not-a-real-secret`).
- Toda conexión a RabbitMQ usa AMQPS (TLS). Nunca se deshabilita ni se
  relaja la verificación del certificado (ni en tests ni "temporalmente").
  Un puerto AMQP en claro solo se acepta en el compose local, marcado como
  solo-desarrollo.
- Mínimo privilegio: un usuario por servicio (`gateway`, `ms-nmap`,
  `ms-analisis`), con permisos `configure`/`write`/`read` acotados por
  regex solo a sus exchanges/colas. Se rechaza cualquier regex amplia
  (`.*`) o un usuario con acceso a la cola de otro servicio.
- La contraseña del usuario administrador nunca se hardcodea fuera del
  ámbito de laboratorio; en un entorno real sale de variable de entorno o
  gestor de secretos.
- Ningún test, script ni config loguea el cuerpo completo de un mensaje
  que pueda contener `ssh_credentials_ref`.
- Las colas no se configuran como almacenamiento de largo plazo (sin TTL
  ni límites que retengan mensajes con credenciales indefinidamente).

## Topología y contratos (bloqueante)

- `rabbitmq/definitions.json` es la única fuente de verdad de nombres:
  vhost, exchanges, colas, bindings, usuarios y permisos. Tests y docs que
  citan un nombre deben coincidir exactamente con ese archivo.
- Nombres según `docs/conventions.md`: exchanges `dominio.acción`
  (`scan.requests`), DLX `<exchange>.dlx`, colas
  `<servicio-consumidor>.<qué-consume>`, DLQ `<cola>.dlq`, routing keys
  que describen el evento y no el destinatario.
- Toda cola de trabajo tiene su dead-letter (`x-dead-letter-exchange`) y,
  si aplica, `x-delivery-limit`, declarados en `arguments` o `policies`,
  nunca implícitos en el código de los tests.
- Un cambio en `contracts/*.schema.json` refleja literalmente la forma que
  ya implementa el productor/consumidor (`ms-nmap` `domain.rs`). Un cambio
  incompatible del contrato es una decisión explícita, documentada en
  `contracts/README.md` y `docs/decisions.md`, no un ajuste silencioso.
- No se crea un exchange nuevo solo porque un consumidor quiere un
  subconjunto de eventos existentes: se usa otro binding con routing key.

## Corrección (bloqueante)

- Nada de `unwrap()`/`expect()`/`panic!()` fuera de tests.
- Errores tipados con `thiserror`; nada de `String` como tipo de error.
- Ninguna llamada bloqueante dentro de código async (`tokio`).

## Tests

- Todo cambio de topología, permisos, TLS o contrato trae su test en el
  mismo commit.
- Los tests de integración corren contra `rabbitmq:management` real vía
  `testcontainers`, marcados `#[ignore = "requiere Docker"]`; nunca mocks
  de RabbitMQ ni instancias de staging/producción.
- Los tests de permisos cubren el caso positivo (el servicio accede a lo
  suyo) y el negativo (acceso denegado a lo ajeno).
- Nombres descriptivos del comportamiento
  (`ms_nmap_user_cannot_publish_to_ms_analisis_queue`).

## Estilo

- `cargo fmt` y `cargo clippy --all-targets -- -D warnings` sin warnings
  (`make verify` corre ambos).
- Sin `println!`/`dbg!` de depuración ni TODOs sin contexto.
- Comentarios solo para explicar un *por qué* no obvio.
- Nombres: `snake_case` para módulos, funciones y variables; `PascalCase`
  para tipos; `UPPER_SNAKE` para constantes.
