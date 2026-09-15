# Exploración: `testcontainers` + `lapin` para tests de integración de la topología RabbitMQ

> Investigación para la feature de tests de integración (Nivel 3 de
> `docs/verification.md`). Fuentes: docs.rs, GitHub (`amqp-rs/lapin`,
> `testcontainers/testcontainers-rs-modules-community`), Docker Hub. Todos los
> fragmentos de código marcados como "fuente real" vienen de fetch directo al
> repositorio/documentación citada; los marcados "ejemplo propuesto" son
> pseudo-código para este repo, no verificado por compilación.

## 1. Qué crate de `testcontainers` usar

Existen dos opciones, **no mutuamente excluyentes**:

### 1a. `testcontainers-modules` con feature `rabbitmq` (recomendado como base)

- Crate: `testcontainers-modules`, feature `rabbitmq`.
- Depende de `testcontainers ^0.27` (mantener versiones alineadas: usar la
  misma versión de `testcontainers` directa que la que trae
  `testcontainers-modules` como dependencia, no mezclar mayor/minor distintos).
- Fuente real del módulo
  (`testcontainers-rs-modules-community/src/rabbitmq/mod.rs`, rama `main`):

  ```rust
  use testcontainers::{core::WaitFor, Image};

  const NAME: &str = "rabbitmq";
  const TAG: &str = "4.2-management";

  #[derive(Debug, Default, Clone)]
  pub struct RabbitMq {
      _priv: (),
  }

  impl Image for RabbitMq {
      fn name(&self) -> &str {
          NAME
      }

      fn tag(&self) -> &str {
          TAG
      }

      fn ready_conditions(&self) -> Vec<WaitFor> {
          vec![WaitFor::message_on_stdout("Server startup complete")]
      }
  }
  ```

  **Importante**: este `struct RabbitMq` es deliberadamente minimalista — no
  expone métodos propios para montar archivos ni fijar variables de entorno.
  Usa la imagen oficial `rabbitmq:4.2-management` de Docker Hub (la misma
  familia de imagen que ya usamos en `docker-compose.yml`, la del ecosistema
  `docker-library/rabbitmq`) y solo define el nombre/tag por defecto y la
  condición de "listo" (`"Server startup complete"` en stdout). Si el tag
  `4.2-management` no coincide con el que fijamos en `docker-compose.yml`,
  hay que sobreescribirlo con `.with_tag(...)` (ver 1b) para no verificar
  contra una versión de RabbitMQ distinta a la de producción/compose.

- Test de ejemplo incluido en el propio crate (fuente real, mismo archivo,
  módulo `tests`) — confirma el patrón end-to-end publish→consume con
  `lapin` 3.x:

  ```rust
  #[tokio::test]
  async fn rabbitmq_produce_and_consume_messages(
  ) -> Result<(), Box<dyn std::error::Error + 'static>> {
      let rabbit_node = rabbitmq::RabbitMq::default().start().await?;
      let amqp_url = format!(
          "amqp://{}:{}",
          rabbit_node.get_host().await?,
          rabbit_node.get_host_port_ipv4(5672).await?
      );
      let connection = Connection::connect(amqp_url.as_str(), ConnectionProperties::default())
          .await
          .unwrap();
      let channel = connection.create_channel().await.unwrap();
      // ... exchange_declare / queue_declare / queue_bind / basic_publish / basic_consume
  }
  ```

  Nótese: llaman a `.exchange_declare("test_exchange", ExchangeKind::Topic, ...)`
  pasando `&str` directamente (no `.into()`) — confirma que en `lapin` 3.x
  los métodos aceptan `impl Into<...>`/`&str` sin conversión explícita.

### 1b. `testcontainers` genérico + `ImageExt` (necesario para montar `definitions.json`)

Como `RabbitMq::default()` no soporta mounts/env vars por sí solo, la pieza
que sí los añade es el trait genérico `ImageExt` de `testcontainers` (crate
base, no del módulo), que se puede encadenar sobre **cualquier** `Image`,
incluido `rabbitmq::RabbitMq`. No hace falta escribir una imagen custom desde
cero: se combina `testcontainers_modules::rabbitmq::RabbitMq` + `ImageExt`.

Métodos relevantes de `ImageExt` (`testcontainers::core::ImageExt`, fuente:
docs.rs):

```rust
fn with_env_var(self, name: impl Into<String>, value: impl Into<String>) -> ContainerRequest<I>;
fn with_mount(self, mount: impl Into<Mount>) -> ContainerRequest<I>;
fn with_tag(self, tag: impl Into<String>) -> ContainerRequest<I>;
fn with_mapped_port(self, host_port: u16, container_port: ContainerPort) -> ContainerRequest<I>;
fn with_container_name(self, name: impl Into<String>) -> ContainerRequest<I>;
```

Y `testcontainers::core::Mount`:

```rust
pub fn bind_mount(host_path: impl Into<String>, container_path: impl Into<String>) -> Self;
```

**Para puertos efímeros del host (recomendado en tests, evita colisiones)**:
simplemente **no** llamar a `.with_mapped_port(...)`. Si no se fija un puerto
de host explícito, `testcontainers` publica el puerto expuesto del
contenedor (5672, 15672 — expuestos vía `EXPOSE` en la imagen oficial) en un
puerto efímero del host asignado por Docker, y se recupera con
`container.get_host_port_ipv4(5672).await?` /
`get_host_port_ipv4(15672).await?`. Esto es justo lo que hace el ejemplo
oficial de 1a. Usar `.with_mapped_port` solo si se necesita fijar un puerto
concreto (no recomendado aquí).

### Combinando ambos: montar `definitions.json` y cargarlo al arrancar

**Dato clave que corrige una asunción del enunciado**: la variable de
entorno `RABBITMQ_LOAD_DEFINITIONS` **no existe** en la imagen oficial
`docker-library/rabbitmq` (la que usa `testcontainers-modules` y la que
probablemente usamos en `docker-compose.yml`). Esa variable es específica de
la imagen **Bitnami** (`bitnami/rabbitmq`), confirmado en su
`README.md` (`RABBITMQ_LOAD_DEFINITIONS` default `no`,
`RABBITMQ_DEFINITIONS_FILE` default `/app/load_definition.json`,
incompatible con `RABBITMQ_SECURE_PASSWORD`). Como este repo usa
`rabbitmq:<tag>-management` (imagen oficial, ver `docs/conventions.md` y
`docs/verification.md`), **no podemos usar `RABBITMQ_LOAD_DEFINITIONS`**.

Para la imagen oficial hay dos formas correctas de cargar definiciones al
arrancar:

**Opción A — `rabbitmq.conf` montado (la forma "canónica", recomendada)**:
monta *dos* archivos: `definitions.json` y un `rabbitmq.conf` con la línea
`management.load_definitions = /etc/rabbitmq/definitions.json`.

```rust
// ejemplo propuesto
use testcontainers::core::{Mount, ImageExt};
use testcontainers_modules::rabbitmq::RabbitMq;

let definitions_path = std::fs::canonicalize("rabbitmq/definitions.json")?;
let conf_path = std::fs::canonicalize("rabbitmq/rabbitmq.conf")?; // contiene:
// management.load_definitions = /etc/rabbitmq/definitions.json

let container = RabbitMq::default()
    .with_mount(Mount::bind_mount(
        definitions_path.to_string_lossy().into_owned(),
        "/etc/rabbitmq/definitions.json",
    ))
    .with_mount(Mount::bind_mount(
        conf_path.to_string_lossy().into_owned(),
        "/etc/rabbitmq/rabbitmq.conf",
    ))
    .start()
    .await?;
```

**Opción B — sin mountear `rabbitmq.conf`, vía `RABBITMQ_SERVER_ADDITIONAL_ERL_ARGS`**:
la imagen oficial sí reconoce esta variable (se pasa como argumentos extra al
nodo Erlang). Evita el segundo mount cuando no queremos gestionar un
`rabbitmq.conf` aparte:

```rust
// ejemplo propuesto
let container = RabbitMq::default()
    .with_mount(Mount::bind_mount(
        definitions_path.to_string_lossy().into_owned(),
        "/etc/rabbitmq/definitions.json",
    ))
    .with_env_var(
        "RABBITMQ_SERVER_ADDITIONAL_ERL_ARGS",
        "-rabbitmq_management load_definitions \"/etc/rabbitmq/definitions.json\"",
    )
    .start()
    .await?;
```

Dado que `docs/conventions.md` ya establece que `rabbitmq/definitions.json`
es la fuente de verdad única y que las políticas van como `arguments`/
`policies` dentro de ese mismo archivo, la Opción A es más alineada: si en
algún momento se necesita `rabbitmq.conf` para otra cosa (p. ej. forzar TLS,
ver `docs/security-scope.md`), ya queda un único lugar (`rabbitmq.conf`
versionado en `rabbitmq/`) en vez de una env var con sintaxis Erlang
incrustada en el código Rust del test. **Decisión sugerida para el
implementer**: usar Opción A y versionar `rabbitmq/rabbitmq.conf` junto a
`rabbitmq/definitions.json`.

Mapeo de puertos management (15672) igual que AMQP: no fijar host port,
usar `container.get_host_port_ipv4(15672).await?` para construir la URL base
de la API HTTP (sección 4).

## 2. Conectar con `lapin` como un usuario de servicio concreto y detectar "permiso denegado"

### Conexión con URI y credenciales de un usuario no-admin

```rust
use lapin::{Connection, ConnectionProperties};

let host = container.get_host().await?;
let port = container.get_host_port_ipv4(5672).await?;
let vhost = "security-app"; // debe ir URL-encoded si tuviera '/' u otros caracteres especiales
let uri = format!("amqp://ms-nmap:{password}@{host}:{port}/{vhost}");

let connection = Connection::connect(&uri, ConnectionProperties::default()).await?;
let channel = connection.create_channel().await?;
```

- `Connection::connect` ya falla (devuelve `Err`) si las credenciales del
  usuario/vhost son inválidas o si el usuario no tiene permiso de `login`
  sobre ese vhost — ahí el error típico es un `lapin::Error::ProtocolError`
  envolviendo un `AMQPError` de conexión (`access_refused` a nivel de
  `connection.open`), o directamente un cierre de conexión durante el
  handshake.

### Qué error da `lapin` cuando el permiso es insuficiente en una operación concreta

Fuente real (`amqp-rs/lapin/src/error.rs`, rama `main`):

```rust
#[non_exhaustive]
pub enum ErrorKind {
    ProtocolError(AMQPError), // "The broker sent an AMQP error (channel or connection level)"
    ParsingError(ParserError),
    SerialisationError(Arc<GenError>),
    IOError(Arc<io::Error>),
    RuntimeShutdownError(Arc<io::Error>),
    InvalidChannel(ChannelId),
    InvalidChannelState(ChannelState, &'static str),
    InvalidConnectionState(ConnectionState),
    // ...
}
```

Con métodos de diagnóstico en `lapin::Error`:
`is_amqp_soft_error()` (error a nivel de canal — cierra el canal, no la
conexión) e `is_amqp_hard_error()` (error a nivel de conexión — cierra toda
la conexión).

El `AMQPError` interno (crate `amq-protocol`, `amq_protocol::protocol::AMQPError`):

```rust
pub enum AMQPError {
    Soft(AMQPSoftError),
    Hard(AMQPHardError),
}
```

con `get_id(&self) -> ShortUInt` (el código numérico AMQP 0-9-1) y
`from_id(id) -> Option<AMQPError>`. El código de interés es
**`AMQPSoftError::ACCESSREFUSED`, id `403`** — es un *soft error*, es decir,
**cierra el canal, no la conexión** (`channel-close` con `reply-code 403`,
`reply-text` tipo `"ACCESS_REFUSED - ..."`). Esto pasa, por ejemplo, cuando
un usuario con permisos solo sobre `ms-nmap.*` intenta `queue_declare`/
`exchange_declare`/`queue_bind`/`basic_publish`/`basic_consume` sobre algo
que no matchea su regex de permisos en `definitions.json`.

**Patrón de assert recomendado para el test negativo**:

```rust
// ejemplo propuesto
let result = channel
    .basic_publish(
        "scan.outcomes", // exchange ajeno a ms-nmap
        "scan.outcome.completed",
        BasicPublishOptions::default(),
        b"{}",
        BasicProperties::default(),
    )
    .await; // el primer await falla al recibir el channel-close del broker

match result {
    Err(lapin::Error(kind, _)) if kind.is_amqp_soft_error() => {
        // esperado: 403 ACCESS_REFUSED — el canal quedó cerrado por el broker
    }
    other => panic!("se esperaba un soft error AMQP (403 ACCESS_REFUSED), llegó: {other:?}"),
}
```

Nota práctica: tras un soft error el **canal** queda inutilizable (hay que
abrir uno nuevo con `connection.create_channel()` para la siguiente
aserción positiva/negativa dentro del mismo test); la **conexión** sigue
viva. Para `basic_publish` sin *publisher confirms* el error de permisos
puede no manifestarse en el primer `.await` (el que entrega el frame al
socket) sino en el **segundo** `.await` de la doble-`await` (`.await?.await?`,
patrón `PublisherConfirm`) o al hacer la siguiente operación sobre el mismo
canal — conviene, en el test negativo, forzar una operación de
confirmación (p. ej. intentar un `queue_declare(passive: true)` sobre algo
ajeno, que sí falla de forma síncrona en el primer `.await`) en vez de
depender únicamente de `basic_publish` para la aserción negativa más clara.

## 3. Verificar pasivamente que un exchange/cola ya existe (sin crearlo)

`passive: true` en `ExchangeDeclareOptions`/`QueueDeclareOptions` le pide al
broker que **compruebe** que el recurso existe con el tipo/argumentos
declarados, sin crearlo ni modificarlo — si no existe (o si existe con otro
tipo), el broker responde con un error a nivel de canal (`NOT_FOUND`, 404,
también soft error).

```rust
use lapin::{options::*, types::FieldTable, ExchangeKind};

// Verifica que "scan.requests" existe como topic exchange, tal cual definitions.json
channel
    .exchange_declare(
        "scan.requests",
        ExchangeKind::Topic,
        ExchangeDeclareOptions { passive: true, ..Default::default() },
        FieldTable::default(),
    )
    .await?; // Err si no existe o no es topic

// Verifica que la cola "ms-nmap.scan-requests" existe
let queue = channel
    .queue_declare(
        "ms-nmap.scan-requests",
        QueueDeclareOptions { passive: true, ..Default::default() },
        FieldTable::default(),
    )
    .await?;
```

Limitación importante: la declaración pasiva de cola confirma **existencia**
pero no expone directamente los `arguments` (p. ej.
`x-dead-letter-exchange`) en la respuesta AMQP estándar de `lapin` de forma
cómoda de inspeccionar — para verificar el binding exacto/routing-key/
argumentos de dead-lettering conviene combinar esto con la API HTTP de
management (sección 4), que sí devuelve el JSON completo de cada recurso.

## 4. API HTTP de management para verificar bindings/routing-keys exactos

Crate recomendado: **`reqwest`** con feature `json` (async, sobre `tokio`,
coherente con la convención del repo de "runtime async, nada de bloqueante
en una tarea async" de `docs/conventions.md` — evita la variante
`blocking`, que además arrastra su propio runtime y podría chocar con el
runtime de `#[tokio::test]`).

```rust
// ejemplo propuesto
let mgmt_port = container.get_host_port_ipv4(15672).await?;
let host = container.get_host().await?;
let base = format!("http://{host}:{mgmt_port}/api");
let vhost_encoded = "security-app"; // "/" en el nombre de vhost iría %2F

let client = reqwest::Client::new();

let bindings: serde_json::Value = client
    .get(format!("{base}/bindings/{vhost_encoded}"))
    .basic_auth("admin", Some(&admin_password)) // usuario admin de definitions.json, solo para verificación
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;

let ms_nmap_binding_exists = bindings
    .as_array()
    .unwrap()
    .iter()
    .any(|b| {
        b["source"] == "scan.requests"
            && b["destination"] == "ms-nmap.scan-requests"
            && b["routing_key"] == "scan.request"
    });
assert!(ms_nmap_binding_exists);
```

Endpoints relevantes:
- `GET /api/exchanges/<vhost>` — lista exchanges con `type`, `durable`, `arguments`.
- `GET /api/queues/<vhost>` — lista colas con `arguments` (incluye
  `x-dead-letter-exchange`, `x-dead-letter-routing-key`, TTL).
- `GET /api/bindings/<vhost>` — lista bindings con `source`, `destination`,
  `destination_type`, `routing_key`, `arguments`.
- `<vhost>` va URL-encoded (el vhost por defecto `/` se codifica `%2F`; el
  vhost de este repo, `security-app`, no necesita encoding por ser
  kebab-case simple).

Cuándo usar esto vs. `lapin` pasivo: la declaración pasiva por AMQP es más
"end-to-end real" (mismo protocolo que usan los servicios productores/
consumidores) pero solo confirma existencia/tipo; la API HTTP es más simple
para aserciones sobre **bindings y routing keys exactos** (que AMQP puro no
expone fácilmente vía `lapin`) y para inspeccionar `arguments` de
dead-lettering sin tener que declarar nada. Conviene usar ambos: AMQP
pasivo para los tests de "el usuario de servicio puede ver/usar su propio
recurso" (coherente con el flujo real), y HTTP management para el test
"la topología cargada coincide exactamente con `definitions.json`" —
idealmente ese segundo test compara el JSON de la API contra el propio
`rabbitmq/definitions.json` parseado, no contra valores hardcodeados
(alineado con la regla de `docs/conventions.md` de que `definitions.json`
es la fuente de verdad única).

**Nota de seguridad**: usar el usuario `admin`/gestión solo para estas
consultas de verificación vía HTTP — nunca las credenciales de un usuario de
servicio (`ms-nmap`, etc.) para la API de management, ya que esos usuarios
no deberían tener acceso a ella (mínimo privilegio, `docs/security-scope.md`).
La contraseña de administración en `definitions.json` es de laboratorio
(ver `docs/conventions.md`), nunca un secreto real, y no debe loggearse el
cuerpo de las respuestas si en algún momento incluyeran datos de mensajes.

## 5. Esqueleto de test completo (pseudo-código, no verificado por compilación)

```rust
// tests/topology_scan_requests.rs
// ejemplo propuesto — no compilado, ilustra la estructura esperada

use std::time::Duration;

use futures::StreamExt;
use lapin::{
    options::{BasicAckOptions, BasicConsumeOptions, BasicPublishOptions},
    types::FieldTable,
    BasicProperties, Connection, ConnectionProperties,
};
use testcontainers::core::{ImageExt, Mount};
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::rabbitmq::RabbitMq;

#[tokio::test]
#[ignore = "requiere Docker"]
async fn ms_nmap_publishes_scan_request_and_it_reaches_its_queue() {
    // 1. Levantar el contenedor con la topología real cargada
    let definitions_path = std::fs::canonicalize("rabbitmq/definitions.json")
        .expect("rabbitmq/definitions.json debe existir");
    let conf_path = std::fs::canonicalize("rabbitmq/rabbitmq.conf")
        .expect("rabbitmq/rabbitmq.conf debe existir");

    let container = RabbitMq::default()
        .with_tag("4.2-management") // igual al tag de docker-compose.yml
        .with_mount(Mount::bind_mount(
            definitions_path.to_string_lossy().into_owned(),
            "/etc/rabbitmq/definitions.json",
        ))
        .with_mount(Mount::bind_mount(
            conf_path.to_string_lossy().into_owned(),
            "/etc/rabbitmq/rabbitmq.conf",
        ))
        .start()
        .await
        .expect("el contenedor RabbitMQ debe arrancar");

    let host = container.get_host().await.expect("host del contenedor");
    let amqp_port = container
        .get_host_port_ipv4(5672)
        .await
        .expect("puerto AMQP mapeado");

    // 2. Conectar como el usuario de servicio ms-nmap (no admin)
    //    La password de laboratorio vive en rabbitmq/definitions.json, se
    //    lee de ahí en el test, nunca hardcodeada por duplicado.
    let ms_nmap_password = load_lab_password_from_definitions("ms-nmap");
    let uri = format!("amqp://ms-nmap:{ms_nmap_password}@{host}:{amqp_port}/security-app");
    let connection = Connection::connect(&uri, ConnectionProperties::default())
        .await
        .expect("ms-nmap debe poder conectar con sus credenciales");
    let channel = connection
        .create_channel()
        .await
        .expect("debe poder abrir canal");

    // 3. Publicar un ScanRequest de ejemplo en el exchange scan.requests
    let payload = br#"{"scan_id":"lab-1","ssh_credentials_ref":"lab-only"}"#;
    channel
        .basic_publish(
            "scan.requests",
            "scan.request",
            BasicPublishOptions::default(),
            payload,
            BasicProperties::default(),
        )
        .await
        .expect("publish debe aceptarse")
        .await
        .expect("el broker debe confirmar el mensaje");

    // 4. Verificar que llega íntegro a ms-nmap.scan-requests
    let mut consumer = channel
        .basic_consume(
            "ms-nmap.scan-requests",
            "test-consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .expect("ms-nmap debe poder consumir su propia cola");

    let delivery = tokio::time::timeout(Duration::from_secs(5), consumer.next())
        .await
        .expect("no debe hacer timeout")
        .expect("debe llegar un mensaje")
        .expect("la entrega no debe traer error de protocolo");

    assert_eq!(delivery.data, payload);
    assert_eq!(delivery.routing_key.as_str(), "scan.request");

    delivery
        .ack(BasicAckOptions::default())
        .await
        .expect("ack debe aceptarse");

    // 5. Cerrar limpio
    channel.close(200, "test terminado").await.ok();
    connection.close(200, "test terminado").await.ok();
}
```

Puntos a respetar según `docs/conventions.md`/`docs/verification.md` al
convertir esto en el test real del implementer:
- Nombre de test descriptivo (`ms_nmap_publishes_scan_request_and_it_reaches_its_queue`
  sigue el patrón `ms_nmap_user_cannot_publish_to_ms_analisis_queue` del ejemplo
  de convenciones, adaptado al caso positivo).
- `#[ignore = "requiere Docker"]` obligatorio.
- Nada de `unwrap()`/`expect()` fuera de tests — este código vive en
  `tests/`, así que `expect(...)` con mensaje descriptivo es aceptable ahí,
  pero si se extraen helpers a `src/lib.rs` (p. ej.
  `load_lab_password_from_definitions`), esos helpers deben devolver
  `Result` con un error de `thiserror`, no `panic!`.
- El payload de ejemplo debe ser un `ScanRequest` de laboratorio (`ssh_credentials_ref`
  ficticio, nunca una credencial real) y además validarse contra
  `contracts/scan-request.schema.json` (Nivel 4 de `docs/verification.md`) —
  no cubierto en este esqueleto, que se centra solo en el transporte.
- El companion "caso negativo" (`ms_nmap_cannot_publish_to_ms_analisis_queue`
  o similar) sigue el mismo arranque de contenedor pero hace lo descrito en
  la sección 2 (assert sobre `is_amqp_soft_error()` / código 403).

## Resumen de decisiones para el implementer

| Pregunta | Respuesta |
|---|---|
| ¿`testcontainers` genérico o módulo dedicado? | Ambos: `testcontainers-modules` (feature `rabbitmq`) da el `Image` base; `testcontainers::core::ImageExt` (`with_mount`, `with_env_var`, `with_tag`) añade lo que el módulo no cubre. |
| ¿Puertos fijos o efímeros? | Efímeros: no llamar `.with_mapped_port`, usar `get_host_port_ipv4(5672)` / `get_host_port_ipv4(15672)` tras `start()`. |
| ¿`RABBITMQ_LOAD_DEFINITIONS`? | **No existe en la imagen oficial** (es de Bitnami). Usar mount de `rabbitmq.conf` con `management.load_definitions = /etc/rabbitmq/definitions.json` (opción recomendada) o `RABBITMQ_SERVER_ADDITIONAL_ERL_ARGS` como alternativa sin segundo archivo. |
| ¿Cómo se ve un permiso denegado en `lapin`? | `lapin::Error` cuyo `ErrorKind::ProtocolError(AMQPError::Soft(AMQPSoftError::ACCESSREFUSED))` — comprobar con `error.kind().is_amqp_soft_error()` (o pattern-matching si se necesita el código 403 exacto). Cierra el canal, no la conexión. |
| ¿Cómo verificar topología sin crearla? | `exchange_declare`/`queue_declare` con `passive: true` — falla (soft error, típicamente 404 NOT_FOUND) si no existe o no coincide el tipo. |
| ¿Alternativa para bindings/routing-keys exactos? | API HTTP de management (`reqwest` async, no `blocking`) contra `GET /api/{exchanges,queues,bindings}/<vhost>`, autenticada con el usuario admin de `definitions.json` (nunca un usuario de servicio). |

## Fuentes consultadas

- https://docs.rs/testcontainers-modules/latest/testcontainers_modules/rabbitmq/index.html
- https://github.com/testcontainers/testcontainers-rs-modules-community (archivo `src/rabbitmq/mod.rs`, rama `main`)
- https://docs.rs/testcontainers/latest/testcontainers/core/trait.ImageExt.html
- https://docs.rs/testcontainers/latest/testcontainers/core/struct.Mount.html
- https://docs.rs/lapin/latest/lapin/
- https://github.com/amqp-rs/lapin (archivo `src/error.rs`, rama `main`)
- https://docs.rs/amq-protocol/0.18.1/amq_protocol/protocol/enum.AMQPError.html
- https://github.com/bitnami/containers/blob/main/bitnami/rabbitmq/README.md (para descartar `RABBITMQ_LOAD_DEFINITIONS` como específico de Bitnami)
- https://codemia.io/knowledge-hub/path/import_broker_definitions_into_dockerized_rabbitmq (patrón `rabbitmq.conf` + `management.load_definitions` para la imagen oficial)
- crates.io: `lapin`, `testcontainers`, `testcontainers-modules`, `reqwest`
