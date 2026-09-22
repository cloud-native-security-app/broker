# Explore: `lapin` + AMQPS (TLS) + `testcontainers` para la feature `tls`

> Investigación de solo-lectura (no toca `Cargo.toml`/`tests/`/`src/`). Todo
> el código de esta nota está verificado contra el **código fuente real**
> (no la documentación resumida de docs.rs, que en varias páginas es
> generada/parafraseada) de las versiones que ya fija este repo:
>
> - `lapin = "4.11.0"` (repo `amqp-rs/lapin`, tag `v4.11.0`)
> - `amq-protocol` (repo `amqp-rs/amq-protocol`, tag `v10.6.3` — el `^10.5`
>   que pide `lapin` 4.11.0 resuelve hoy a esa versión)
> - `tcp-stream` (repo `amqp-rs/tcp-stream`, tag `v0.34.14` — resuelve del
>   `^0.34.5` que pide `amq-protocol-tcp`)
> - `rustls-connector` (repo `amqp-rs/rustls-connector`, tag `v0.23.8`)
> - `testcontainers = "0.27"` (repo `testcontainers/testcontainers-rs`, tag
>   `0.27.0`)
> - `testcontainers-modules = "0.15.0"` feature `rabbitmq` (repo
>   `testcontainers/testcontainers-rs-modules-community`, tag `v0.15.0`)
> - Imagen `rabbitmq:4.3.5-management` (repo `docker-library/rabbitmq`,
>   Dockerfiles `4.3/ubuntu/Dockerfile` y `4.3/ubuntu/management/Dockerfile`
>   en `master`)
>
> Cada afirmación no trivial cita el archivo/línea exactos leídos. Cuando
> una fuente (típicamente un resumen de docs.rs vía WebFetch) resultó
> incompleta o ambigua, se cruzó con el código fuente en GitHub — se anota
> explícitamente cuándo pasó eso.

## Resumen ejecutivo (para el implementer)

1. **No hace falta tocar las features de `lapin` en `Cargo.toml`.** El
   `lapin = "4.11.0"` que ya está en el repo trae, por defecto
   (`default = ["rustls", "default-runtime"]`), todo lo necesario:
   backend `rustls` + `rustls-platform-verifier` + proveedor criptográfico
   `rustls--aws_lc_rs` + runtime `tokio`. Confiar en la CA autofirmada de
   laboratorio se hace en **runtime**, pasando su PEM en
   `OwnedTLSConfig.cert_chain` — no es una feature de compilación.
2. La API exacta es `lapin::Connection::connect_with_config(uri, options,
   OwnedTLSConfig { identity: None, cert_chain: Some(ca_pem) },
   lapin::runtime::default_runtime()?)`. No hace falta construir un
   `Connector` TLS manualmente con `tokio-rustls`/`tokio-native-tls`:
   `lapin` ya expone ese punto de extensión.
3. Para el contenedor de test: **no hace falta llamar a ningún método
   "with_exposed_port"** (no existe tal método en `testcontainers` 0.27 —
   ver más abajo por qué). El puerto 5671 se publica solo, exactamente
   igual que ya pasa con 5672 y 15672 en `tests/common/mod.rs` — es
   responsabilidad del `Dockerfile` de la imagen oficial (`EXPOSE 5671`) +
   el comportamiento por defecto de `testcontainers` (`publish_all_ports =
   true` cuando no se pidió un mapeo de puerto explícito). Los certs de
   laboratorio se montan con el mismo patrón `Mount::bind_mount` que ya
   usa `tests/common/mod.rs` para `definitions.json`/`rabbitmq.conf`.
4. Test positivo: conectar por `amqps://` con la CA de laboratorio y hacer
   una declaración pasiva de un exchange ya existente (p. ej.
   `scan.requests`) — falla si el exchange no existe, así que sirve como
   prueba de que la conexión TLS realmente opera sobre la topología real,
   no solo que "no dio error".
5. Test negativo: **la forma más determinista y menos dependiente del
   comportamiento interno de `lapin`/rustls es un `tokio::net::TcpStream`
   crudo** que abre el socket al puerto 5671 y escribe el *protocol
   header* AMQP 0-9-1 en claro (8 bytes exactos, verificados en el código
   fuente de `amq-protocol`, ver §5). RabbitMQ, en ese puerto, espera un
   `ClientHello` TLS — al recibir texto plano, el handshake TLS del lado
   servidor falla y el socket se cierra sin que llegue ninguna respuesta
   AMQP válida. También se documenta la alternativa de usar `lapin` con
   una URI `amqp://` (sin “s”) apuntando al puerto 5671, que es más corta
   pero menos determinista (puede fallar con error o con timeout según la
   pila TLS/Erlang exacta) — se recomienda usar ambas: la cruda como
   aserción principal, la de `lapin` como comentario/nota, no como test
   separado obligatorio.

---

## 1. Qué feature de `lapin` habilita TLS

### 1.1 Cargo features de `lapin` 4.11.0

Del `Cargo.toml` real de `lapin` v4.11.0
(<https://github.com/amqp-rs/lapin/blob/v4.11.0/Cargo.toml>):

```toml
[features]
default                   = ["rustls", "default-runtime"]
default-runtime           = ["tokio"]

native-tls                = ["amq-protocol/native-tls"]
openssl                   = ["amq-protocol/openssl"]
rustls                    = ["amq-protocol/rustls"]
rustls-platform-verifier  = ["amq-protocol/rustls-platform-verifier"]
rustls-native-certs       = ["amq-protocol/rustls-native-certs"]
rustls-webpki-roots-certs = ["amq-protocol/rustls-webpki-roots-certs"]
vendored-openssl          = ["amq-protocol/vendored-openssl"]

# rustls crypto providers. Choose at least one. Otherwise, runtime errors.
rustls--aws_lc_rs         = ["amq-protocol/rustls--aws_lc_rs"] # default, but doesn't build everywhere
rustls--ring              = ["amq-protocol/rustls--ring"] # more compatible, (e.g., easily builds on Windows)
```

`lapin` no implementa TLS él mismo: todas estas features solo activan
features equivalentes de su dependencia `amq-protocol` (`^10.5`), que a su
vez las reenvía a `amq-protocol-tcp`, que a su vez reenvía a `tcp-stream`
(la librería que de verdad hace el `TcpStream::connect` + handshake TLS).
Cadena confirmada leyendo, en orden:

- `amq-protocol/protocol/Cargo.toml` (tag `v10.6.3`) — `default =
  ["rustls"]`, y cada feature reenvía a `amq-protocol-tcp/<misma-feature>`.
- `amq-protocol/tcp/Cargo.toml` (paquete `amq-protocol-tcp`, tag
  `v10.6.3`) — aquí está la pieza importante que **no es obvia leyendo
  solo el `Cargo.toml` de `lapin`**:

  ```toml
  [features]
  default                   = ["rustls", "tokio"]
  rustls                    = ["rustls-platform-verifier", "rustls--aws_lc_rs"]
  rustls-platform-verifier  = ["rustls-common", "tcp-stream/rustls-platform-verifier"]
  rustls-native-certs       = ["rustls-common", "tcp-stream/rustls-native-certs"]
  rustls-webpki-roots-certs = ["rustls-common", "tcp-stream/rustls-webpki-roots-certs"]
  rustls--aws_lc_rs         = ["tcp-stream/rustls--aws_lc_rs"]
  rustls--ring              = ["tcp-stream/rustls--ring"]
  native-tls                = ["tcp-stream/native-tls-futures"]
  openssl                   = ["tcp-stream/openssl-futures"]
  ```

  Es decir: **la feature `rustls` de `lapin` YA activa
  `rustls-platform-verifier` y el proveedor criptográfico
  `rustls--aws_lc_rs` por transitividad**. El `Cargo.toml` actual de este
  repo (`lapin = "4.11.0"`, sin `default-features = false` ni lista de
  `features`) ya tiene:
  - backend TLS: rustls
  - verificador de certs "base": `rustls-platform-verifier` (verifica
    contra el almacén de confianza de la plataforma — SO/webpki, según
    plataforma)
  - proveedor criptográfico: `aws_lc_rs`
  - runtime: `tokio` (vía `default-runtime`)

### 1.2 ¿native-tls o rustls? ¿Hace falta cambiar algo?

**No hace falta cambiar features.** `rustls` (el default de `lapin`) es
más simple para este caso que `native-tls` por dos razones concretas:

- `native-tls` delega en la librería TLS del sistema operativo
  (OpenSSL/Schannel/Security.framework según plataforma) — para confiar
  en una CA de laboratorio ahí normalmente hay que instalarla en el
  almacén de confianza del SO/contenedor donde corren los tests (más
  pasos, no reproducible igual en cualquier máquina/CI).
- `rustls` (vía `tcp-stream`, ver §2) permite pasar el PEM de la CA de
  laboratorio **directamente como dato en memoria** (`OwnedTLSConfig.cert_chain:
  Option<String>`) sin tocar ningún almacén de confianza del sistema — el
  proceso de test simplemente confía en esa CA además de las del
  verificador de plataforma. Es la opción con menos partes móviles para
  un contenedor efímero de `testcontainers`.

No hace falta añadir `rustls-native-certs` ni `rustls-webpki-roots-certs`
tampoco: esas features solo importan si además quisiéramos validar contra
CAs públicas (no es el caso — es un cert de laboratorio autofirmado). El
`rustls-platform-verifier` que ya viene por defecto **no estorba**: al
añadir la CA de laboratorio vía `cert_chain`, esa CA se suma como "extra
root" al verificador existente (ver exactamente cómo en §2.3) — no hace
falta desactivar ni sustituir el verificador de plataforma.

**Conclusión para `Cargo.toml`:** `lapin = "4.11.0"` tal cual está hoy en
el repo es suficiente. No se necesita ninguna feature nueva.

---

## 2. API exacta para conectar por AMQPS confiando en la CA de laboratorio

### 2.1 El trait `Connect` y los métodos de `Connection`

Código real de `lapin` v4.11.0, `src/connection.rs`
(<https://github.com/amqp-rs/lapin/blob/v4.11.0/src/connection.rs>):

```rust
impl Connection {
    /// Conecta con el runtime y la config TLS por defecto.
    pub async fn connect(uri: &str, options: ConnectionProperties) -> Result<Self> {
        Connect::connect(uri, options).await
    }

    /// Conecta con un runtime explícito y la config TLS por defecto
    /// (`OwnedTLSConfig::default()`, i.e. sin CA/identity custom).
    pub async fn connect_with_runtime<RK: RuntimeKit + Send + Sync + Clone + 'static>(
        uri: &str,
        options: ConnectionProperties,
        runtime: Runtime<RK>,
    ) -> Result<Self> {
        uri.connect_with_config(options, OwnedTLSConfig::default(), runtime).await
    }

    /// Conecta con runtime Y config TLS explícitos — ESTE es el método
    /// que necesitamos para pasar la CA de laboratorio.
    pub async fn connect_with_config<RK: RuntimeKit + Send + Sync + Clone + 'static>(
        uri: &str,
        options: ConnectionProperties,
        config: OwnedTLSConfig,
        runtime: Runtime<RK>,
    ) -> Result<Self> {
        uri.connect_with_config(options, config, runtime).await
    }

    // + connect_uri / connect_uri_with_runtime / connect_uri_with_config:
    // mismos 4 métodos pero recibiendo un `AMQPUri` ya parseado en vez de
    // `&str`.
}

/// Extension trait que permite a los tipos URI (`&str`, `AMQPUri`) abrir
/// una `Connection` directamente.
pub trait Connect {
    async fn connect(self, options: ConnectionProperties) -> Result<Connection>
    where Self: Sized,
    {
        self.connect_with_config(
            options,
            OwnedTLSConfig::default(),
            crate::runtime::default_runtime()?, // ver 2.2
        ).await
    }

    async fn connect_with_config<RK: RuntimeKit + Send + Sync + Clone + 'static>(
        self,
        options: ConnectionProperties,
        config: OwnedTLSConfig,
        runtime: Runtime<RK>,
    ) -> Result<Connection>
    where Self: Sized;
}
```

(La firma exacta de `Connect::connect`'s implementación por defecto usa
`crate::runtime::default_runtime()?` — confirmado leyendo el cuerpo
completo del trait en el mismo archivo.)

**No existe** ningún `TlsAdapter` público ni hace falta construir un
`Connector` de `tokio-rustls`/`tokio-native-tls` a mano: el punto de
extensión que expone `lapin` para "URI + TLS config custom" es
exactamente `Connection::connect_with_config` /
`Connection::connect_uri_with_config`, recibiendo un `OwnedTLSConfig`.

### 2.2 `OwnedTLSConfig` — de dónde sale y qué campos tiene

`lapin::tcp` es un re-export: `lapin/src/lib.rs` hace
`pub use amq_protocol::{ tcp::{self, AsyncTcpStream}, ... };` y a su vez
`amq-protocol/protocol/src/lib.rs` hace `pub use amq_protocol_tcp as tcp;`,
que a su vez re-exporta de `tcp-stream`:

```rust
// amq-protocol-tcp/src/lib.rs (amq-protocol tag v10.6.3, tcp/src/lib.rs)
pub use tcp_stream::{
    AsyncTcpStream, HandshakeError, HandshakeResult, Identity, MidHandshakeTlsStream,
    OwnedIdentity, OwnedTLSConfig, TLSConfig, TcpStream,
};
```

Y la definición real, en `tcp-stream` v0.34.14, `src/lib.rs`
(<https://github.com/amqp-rs/tcp-stream/blob/v0.34.14/src/lib.rs#L192>):

```rust
/// Holds extra TLS configuration
#[derive(Clone, Default, Debug, PartialEq)]
pub struct OwnedTLSConfig {
    /// Use for client certificate authentication
    pub identity: Option<OwnedIdentity>,
    /// The custom certificates chain in PEM format
    pub cert_chain: Option<String>,
}
```

Para nuestro caso (solo confiar en la CA de laboratorio del **servidor**,
sin autenticación de cliente por certificado):

```rust
use lapin::tcp::OwnedTLSConfig;

let ca_pem = std::fs::read_to_string(ca_cert_path)
    .expect("el PEM de la CA de laboratorio debe existir");

let tls_config = OwnedTLSConfig {
    identity: None,        // no hacemos mTLS, solo verificamos al servidor
    cert_chain: Some(ca_pem), // PEM de la CA (o cadena) de laboratorio
};
```

`identity`/`OwnedIdentity` es exclusivamente para autenticación de
**cliente** por certificado (mTLS: PKCS#12 o PKCS#8) — no aplica aquí,
donde el cliente se autentica por usuario/contraseña AMQP (ya lo hace
`ConnectionProperties`/la URI) y solo necesitamos validar el cert del
*servidor*.

### 2.3 Qué hace `cert_chain` exactamente con rustls (por qué no rompe el verificador de plataforma)

Confirmado en `tcp-stream` v0.34.14, `src/rustls_impl.rs`
(<https://github.com/amqp-rs/tcp-stream/blob/v0.34.14/src/rustls_impl.rs>):

```rust
fn select_connector_config() -> io::Result<RustlsConnectorConfig> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "rustls-platform-verifier")] {
            Ok(RustlsConnectorConfig::new_with_platform_verifier())
        } else if #[cfg(feature = "rustls-native-certs")] {
            RustlsConnectorConfig::new_with_native_certs()
        } else if #[cfg(feature = "rustls-webpki-roots-certs")] {
            Ok(RustlsConnectorConfig::new_with_webpki_root_certs())
        } else {
            Ok(RustlsConnectorConfig::default())
        }
    }
}

fn update_rustls_config(c: &mut RustlsConnectorConfig, config: &TLSConfig<'_,'_,'_>) -> io::Result<()> {
    if let Some(cert_chain) = config.cert_chain {
        let mut cert_chain = io::BufReader::new(cert_chain.as_bytes());
        let certs = CertificateDer::pem_reader_iter(&mut cert_chain)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        c.add_parsable_certificates(certs); // <-- aquí se AÑADEN, no reemplazan
    }
    Ok(())
}
```

Y en `rustls-connector` v0.23.8, `src/lib.rs`
(<https://github.com/amqp-rs/rustls-connector/blob/v0.23.8/src/lib.rs>), el
método `add_parsable_certificates` guarda los certs en `self.store`, y al
construir el verificador final:

```rust
if self.platform_verifier {
    // ...
    rustls_platform_verifier::Verifier::new_with_extra_roots(self.store, provider)
    // self.store == la(s) CA(s) de laboratorio que añadimos vía cert_chain
} else {
    let mut store = RootCertStore::empty();
    let (_, ignored) = store.add_parsable_certificates(self.store);
    // ...
}
```

Es decir: dado que la feature `rustls` de `lapin` activa
`rustls-platform-verifier` por defecto (§1.1), pasar
`OwnedTLSConfig.cert_chain = Some(ca_pem)` hace que
`rustls_platform_verifier::Verifier::new_with_extra_roots(...)` reciba
nuestra CA de laboratorio como **raíz de confianza adicional** junto a las
de la plataforma. Resultado práctico: el cert autofirmado de laboratorio
queda confiado sin desactivar ni tocar la verificación normal.

### 2.4 El runtime: `lapin::runtime::default_runtime()`

`connect_with_config` pide un `Runtime<RK>` explícito. Con el
`default-runtime = ["tokio"]` que ya trae `lapin` por defecto, se obtiene
con la función pública `lapin::runtime::default_runtime()`, confirmada en
`lapin` v4.11.0 `src/runtime.rs`:

```rust
// lapin::runtime (pub mod runtime; en src/lib.rs)
pub fn default_runtime() -> Result<DefaultRuntime> {
    // con feature "tokio" (el default-runtime de este repo):
    #[cfg(not(test))]
    return Ok(async_rs::Runtime::tokio_current());
}
```

Devuelve `Result<DefaultRuntime>`, así que se propaga con `?` igual que
cualquier otro error de `lapin::Result`.

### 2.5 Ejemplo completo — conectar por AMQPS con la CA de laboratorio

```rust
use lapin::{tcp::OwnedTLSConfig, Connection, ConnectionProperties};

async fn connect_amqps_lab(
    host: &str,
    port: u16,
    vhost: &str,
    user: &str,
    password: &str,
    ca_pem_path: &std::path::Path,
) -> lapin::Result<Connection> {
    let ca_pem = std::fs::read_to_string(ca_pem_path)
        .unwrap_or_else(|err| panic!("no se pudo leer la CA de laboratorio: {err}"));

    let uri = format!("amqps://{user}:{password}@{host}:{port}/{vhost}");

    let tls_config = OwnedTLSConfig {
        identity: None,
        cert_chain: Some(ca_pem),
    };

    Connection::connect_with_config(
        &uri,
        ConnectionProperties::default(),
        tls_config,
        lapin::runtime::default_runtime()?,
    )
    .await
}
```

Nota sobre la URI: el esquema (`amqp://` vs `amqps://`) es lo único que
decide si `lapin` intenta TLS — confirmado en `amq-protocol-tcp` v10.6.3,
`src/lib.rs`, `impl AMQPUriTcpExt for AMQPUri`:

```rust
let stream = match self.scheme {
    AMQPScheme::AMQP => stream,                               // sin TLS
    AMQPScheme::AMQPS => stream.into_tls(&self.authority.host, config)?, // con TLS
};
```

Esto es relevante también para el punto 5 (más abajo): apuntar una URI
`amqp://` (no `amqps://`) al puerto 5671 hace que `lapin` NO intente TLS
en absoluto y mande el protocolo AMQP en claro — es la base de la
alternativa "con `lapin`" documentada en §5.2.

---

## 3. Montar los certs de laboratorio en el contenedor de `testcontainers` y exponer el puerto 5671

### 3.1 Montar los ficheros — mismo patrón que ya usa `tests/common/mod.rs`

`tests/common/mod.rs` ya monta `definitions.json` y `rabbitmq.conf` así:

```rust
RabbitMq::default()
    .with_tag(RABBITMQ_TAG)
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
    ...
```

`Mount::bind_mount` es genérico: acepta cualquier ruta host → ruta
contenedor. Para los certs de laboratorio, se añaden exactamente igual
(las rutas de destino dependen de lo que declare `rabbitmq.conf` vía
`ssl_options.cacertfile`/`certfile`/`keyfile` — eso lo define/documenta el
otro explorador, `explore_rabbitmq_tls.md`; aquí solo el mecanismo de
montaje del lado `testcontainers`, que es idéntico sea cual sea la ruta
elegida):

```rust
let tls_dir = repo_root().join("rabbitmq/tls"); // p. ej.

RabbitMq::default()
    .with_tag(RABBITMQ_TAG)
    .with_mount(Mount::bind_mount(
        definitions_path.to_string_lossy().into_owned(),
        "/etc/rabbitmq/definitions.json",
    ))
    .with_mount(Mount::bind_mount(
        conf_path.to_string_lossy().into_owned(),
        "/etc/rabbitmq/rabbitmq.conf",
    ))
    .with_mount(Mount::bind_mount(
        tls_dir.join("ca.pem").to_string_lossy().into_owned(),
        "/etc/rabbitmq/tls/ca.pem",
    ))
    .with_mount(Mount::bind_mount(
        tls_dir.join("server-cert.pem").to_string_lossy().into_owned(),
        "/etc/rabbitmq/tls/server-cert.pem",
    ))
    .with_mount(Mount::bind_mount(
        tls_dir.join("server-key.pem").to_string_lossy().into_owned(),
        "/etc/rabbitmq/tls/server-key.pem",
    ))
    .start()
    .await
    .expect("el contenedor RabbitMQ con TLS debe arrancar");
```

`ImageExt`/`Mount` no distinguen "tipo" de fichero — el mismo
`Mount::bind_mount(host_path, container_path)` sirve para JSON, `.conf` o
`.pem`. `Mount` implementa `From` hacia el tipo que pide `with_mount`, tal
como ya está en uso (`testcontainers` 0.27,
`testcontainers::core::{ImageExt, Mount}` — mismos imports que ya hace
`tests/common/mod.rs`, no hace falta nada nuevo).

### 3.2 El puerto 5671: no hace falta "exponerlo" — ya se publica solo

Esto merece explicación porque **no es obvio** y una primera lectura de
docs.rs puede llevar a error (un resumen automático de la página de
`Image` en docs.rs afirmó, incorrectamente respecto al comportamiento real
del código, que los puertos `EXPOSE` del Dockerfile "no se publican
automáticamente" — se verificó contra el código fuente real y **eso es
falso** para el caso por defecto que ya usa este repo). Cadena de
evidencia:

1. **`testcontainers-modules` no declara ningún puerto explícito para
   `RabbitMq`.** El `Image` de `testcontainers-modules` v0.15.0,
   `src/rabbitmq/mod.rs`
   (<https://github.com/testcontainers/testcontainers-rs-modules-community/blob/v0.15.0/src/rabbitmq/mod.rs>):

   ```rust
   impl Image for RabbitMq {
       fn name(&self) -> &str { NAME }
       fn tag(&self) -> &str { TAG }
       fn ready_conditions(&self) -> Vec<WaitFor> {
           vec![WaitFor::message_on_stdout("Server startup complete")]
       }
       // NO sobreescribe expose_ports() -> usa el default (vacío)
   }
   ```

   El propio ejemplo del `mod.rs` conecta a 5672 con
   `get_host_port_ipv4(5672)` sin llamar nunca a nada tipo
   "with_exposed_port" — igual que ya hace `tests/common/mod.rs` de este
   repo con 5672 y 15672.

2. **`testcontainers` 0.27 no tiene ningún método `with_exposed_port` en
   `ImageExt`.** La lista completa de métodos de `ImageExt` en
   `testcontainers` 0.27.0 (verificada contra docs.rs) no incluye tal
   método — lo que existe es `with_mapped_port(host_port, container_port)`
   (para fijar un puerto de HOST concreto, no lo que queremos: queremos
   puerto efímero, igual que 5672/15672) y
   `with_exposed_host_port(port)`/`with_exposed_host_ports(...)`, que es
   una feature **completamente distinta** (`host-port-exposure`): permite
   que el contenedor alcance un servicio que corre en el HOST vía
   `host.testcontainers.internal` — lo contrario de lo que necesitamos, y
   no aplica aquí.

3. **El comportamiento real de publicación de puertos** está en
   `testcontainers` 0.27.0, `src/runners/async_runner.rs` (líneas ~258–296,
   confirmado leyendo el archivo fuente completo,
   <https://github.com/testcontainers/testcontainers-rs/blob/0.27.0/testcontainers/src/runners/async_runner.rs>):

   ```rust
   // ports
   if container_req.ports().is_some() {
       // ... solo si se llamó a with_mapped_port explícitamente
   } else if !is_container_networked {
       config.host_config = config.host_config.map(|mut host_config| {
           host_config.publish_all_ports = Some(true); // == docker run -P
           host_config.port_bindings = Some(HashMap::new());
           host_config
       });
   }
   ```

   Cuando no se llamó a `with_mapped_port` (que es exactamente el caso de
   `tests/common/mod.rs` hoy, y seguiría siéndolo si se añade TLS sin
   tocar esa parte), `testcontainers` arranca el contenedor con
   `publish_all_ports = true`, el equivalente exacto de `docker run -P`:
   Docker publica, a puertos de host aleatorios, **todos** los puertos que
   el propio `Dockerfile` de la imagen declaró con `EXPOSE` — sin que
   `testcontainers` necesite enumerarlos.

4. **El `Dockerfile` oficial de `rabbitmq:4.3.5-management` sí declara
   `EXPOSE 5671`.** Confirmado en `docker-library/rabbitmq`, rama
   `master`:
   - `4.3/ubuntu/Dockerfile` (imagen base):
     `EXPOSE 4369 5671 5672 15691 15692 25672`
   - `4.3/ubuntu/management/Dockerfile` (`FROM rabbitmq:4.3`, añade el
     plugin de management): `EXPOSE 15671 15672`

   La imagen final `rabbitmq:4.3.5-management` hereda `EXPOSE`s de ambas
   capas → incluye 5671.

**Conclusión:** con `publish_all_ports = true` (comportamiento por
defecto de `testcontainers` cuando no se fija un puerto explícito) +
`EXPOSE 5671` ya presente en el Dockerfile de la imagen oficial, el
puerto TLS queda publicado a un puerto de host efímero exactamente igual
que 5672/15672 ya lo están hoy. Basta con:

```rust
let tls_port = container
    .get_host_port_ipv4(5671)
    .await
    .expect("puerto AMQPS (5671) mapeado");
```

sin ningún cambio adicional en cómo se construye `RabbitMq::default()...`
más allá de los `with_mount` de los certs (§3.1). Si en algún momento se
quisiera fijar el puerto 5671 del host explícitamente (no recomendado
para tests — rompe el aislamiento entre ejecuciones paralelas), sería con
`.with_mapped_port(5671, 5671.tcp())` (`ContainerPort` implementa
`From<u16>`/tiene el helper `.tcp()` en `testcontainers::core::ContainerPort`),
pero **no hace falta** para este caso.

### 3.3 Construir la URI AMQPS de test

Análogo a `amqp_uri` que ya existe en `tests/common/mod.rs`, pero con
`amqps://` y el puerto 5671:

```rust
pub async fn amqps_uri(container: &ContainerAsync<RabbitMq>, user: &str, password: &str) -> String {
    let host = container.get_host().await.expect("host del contenedor");
    let port = container
        .get_host_port_ipv4(5671)
        .await
        .expect("puerto AMQPS (5671) mapeado");
    format!("amqps://{user}:{password}@{host}:{port}/{VHOST}")
}
```

---

## 4. Test positivo: AMQPS con la CA de laboratorio + operar sobre la topología real

Usando `connect_amqps_lab` de §2.5 + `amqps_uri`/puerto de §3.3, y
declarando pasivamente un exchange que la feature `topology_definition`
ya garantiza que existe (p. ej. `scan.requests` — ver
`rabbitmq/definitions.json`):

```rust
use lapin::options::ExchangeDeclareOptions;
use lapin::types::FieldTable;
use lapin::ExchangeKind;

#[tokio::test]
#[ignore = "requiere Docker"]
async fn amqps_connection_with_lab_ca_operates_on_real_topology() {
    let container = common::start_broker_with_tls().await; // análogo a start_broker(), + certs montados
    let ca_pem_path = repo_root().join("rabbitmq/tls/ca.pem");

    let connection = common::connect_amqps_lab(
        &container.get_host().await.unwrap(),
        container.get_host_port_ipv4(5671).await.unwrap(),
        VHOST,
        lab_credentials::MS_NMAP_USER,
        lab_credentials::MS_NMAP_PASSWORD,
        &ca_pem_path,
    )
    .await
    .unwrap_or_else(|err| panic!("la conexión AMQPS con la CA de laboratorio debe funcionar: {err}"));

    let channel = connection.create_channel().await.expect("abrir canal");

    // Declaración PASIVA: falla si el exchange no existe con ese nombre/tipo
    // exactos -> confirma que estamos operando sobre la topología real de
    // rabbitmq/definitions.json, no solo que el TLS "no dio error".
    channel
        .exchange_declare(
            "scan.requests",
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                passive: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .expect("scan.requests debe existir — declaración pasiva sobre AMQPS");
}
```

`ExchangeDeclareOptions.passive: Boolean` confirmado en `lapin` v4.11.0,
`src/generated/channel.rs` (código generado desde el spec AMQP 0-9-1,
comentario: *"Verify that the exchange or queue exists without creating or
modifying it"*).

Nota: quien usa esta conexión con `ms-nmap` para declarar pasivamente
`scan.requests` está dentro de lo que sus permisos permiten
(`configure`/`read` sobre ese exchange, según `docs/conventions.md`/
`definitions.json`) — usar el usuario de servicio correcto en vez de
`lab-admin` para este test mantiene la prueba honesta con el resto de la
suite (`tests/permissions.rs` ya sigue ese mismo criterio).

---

## 5. Test negativo: una conexión SIN TLS al puerto 5671 debe rechazarse

### 5.1 Recomendado: `tokio::net::TcpStream` crudo + protocol header AMQP 0-9-1 en claro

Esta es la alternativa que se pidió evaluar, y es la que se recomienda
como aserción principal porque no depende de cómo `lapin`/rustls decidan
reportar el fallo (error inmediato vs. timeout vs. panic interno) — solo
depende del comportamiento TCP/TLS del propio RabbitMQ, que es lo que
realmente queremos probar.

El *protocol header* AMQP 0-9-1 que cualquier cliente (incluido `lapin`)
manda en claro nada más abrir la conexión TCP son 8 bytes exactos,
confirmados leyendo el generador real usado por `lapin`
(`amq-protocol` v10.6.3, `protocol/src/frame/generation.rs` +
`protocol/src/generated.rs`):

```rust
// protocol/src/generated.rs (amq-protocol v10.6.3)
pub const NAME: &str = "AMQP";
pub const MAJOR_VERSION: ShortShortUInt = 0;
pub const MINOR_VERSION: ShortShortUInt = 9;
pub const REVISION: ShortShortUInt = 1;

// protocol/src/frame/generation.rs
fn gen_protocol_header<W: Write>(version: ProtocolVersion) -> impl SerializeFn<W> {
    tuple((
        slice(metadata::NAME.as_bytes()),   // b"AMQP"
        gen_short_short_uint(0),            // 0x00 (protocol id)
        gen_protocol_version(version),      // major, minor, revision
    ))
}
```

→ Los 8 bytes en claro son `b"AMQP\x00\x00\x09\x01"`
(`0x41 0x4D 0x51 0x50 0x00 0x00 0x09 0x01`). Un servidor AMQP en texto
plano respondería con un frame `Connection.Start` (`method`, empezando
con el byte `0x01` de tipo *method*). En el puerto TLS, RabbitMQ espera un
`ClientHello` TLS — al recibir esto en claro, el handshake TLS del lado
servidor falla y Erlang/RabbitMQ cierra el socket sin enviar ninguna
respuesta AMQP:

```rust
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Bytes exactos del protocol header AMQP 0-9-1 que envía cualquier
/// cliente (incluido `lapin`) nada más abrir la conexión — ver
/// `amq-protocol` v10.6.3 `protocol/src/generated.rs` +
/// `protocol/src/frame/generation.rs`.
const AMQP_0_9_1_PROTOCOL_HEADER: &[u8] = b"AMQP\x00\x00\x09\x01";

#[tokio::test]
#[ignore = "requiere Docker"]
async fn plaintext_connection_to_tls_port_is_rejected() {
    let container = common::start_broker_with_tls().await;
    let host = container.get_host().await.expect("host del contenedor");
    let tls_port = container
        .get_host_port_ipv4(5671)
        .await
        .expect("puerto AMQPS (5671) mapeado");

    let mut socket = TcpStream::connect((host.to_string().as_str(), tls_port))
        .await
        .expect("el socket TCP crudo sí debe poder abrirse (5671 escucha)");

    // Si esto falla al escribir (broken pipe / connection reset), ya es
    // evidencia de rechazo -- se tolera como caso válido, no se hace
    // `expect` aquí.
    let write_result = socket.write_all(AMQP_0_9_1_PROTOCOL_HEADER).await;

    let mut buf = [0u8; 8];
    let read_result = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buf)).await;

    match (write_result, read_result) {
        // El write falló (conexión ya cerrada por el servidor al ver
        // texto plano donde esperaba un TLS ClientHello).
        (Err(_), _) => {}
        // El write funcionó pero el read se agotó -> el servidor no
        // completó ningún handshake AMQP (se quedó esperando TLS y nunca
        // respondió) -- también cuenta como rechazo del handshake AMQP en
        // claro.
        (Ok(_), Err(_timeout)) => {}
        // El read terminó: o bien Ok(0) (EOF -- el servidor cerró el
        // socket) o un Err de IO (reset) -- ambos son el resultado
        // esperado. Si en cambio llegan bytes Y esos bytes empiezan con
        // el byte de tipo `method` (0x01) de un `Connection.Start` real,
        // el test SÍ debe fallar: significaría que el puerto 5671 habló
        // AMQP en claro.
        (Ok(_), Ok(Ok(0))) => {}
        (Ok(_), Ok(Err(_io_err))) => {}
        (Ok(_), Ok(Ok(n))) => {
            panic!(
                "el puerto 5671 respondió {n} bytes a un handshake AMQP en \
                 claro en vez de cerrar la conexión/fallar el TLS: {:?}",
                &buf[..n]
            );
        }
    }
}
```

Este patrón cubre los tres desenlaces de red que puede dar un handshake
TLS fallido visto desde un socket TCP crudo (RST inmediato, FIN/EOF, o
simplemente ningún dato — que se corta con el `timeout`), y solo falla el
test si, de forma inequívoca, el servidor efectivamente respondió AMQP en
claro (lo único que probaría que TLS NO se está exigiendo en ese puerto).

### 5.2 Alternativa (más corta, menos determinista): `lapin` con URI `amqp://` contra el puerto 5671

Como se documentó en §2.5, `lapin` decide TLS solo por el esquema de la
URI. Apuntar una URI **`amqp://`** (sin “s”) al puerto 5671 hace que
`lapin` abra el TCP y mande el protocol header en claro exactamente igual
que el test crudo de arriba — con la ventaja de reusar `Connection::connect`
tal cual, pero con la desventaja de que el `Result`/comportamiento exacto
(¿error inmediato, o se queda esperando la respuesta de `Connection.Start`
hasta el timeout de `lapin`?) depende de detalles internos de la máquina
de estados de conexión de `lapin` que no están documentados como
contrato estable:

```rust
#[tokio::test]
#[ignore = "requiere Docker"]
async fn lapin_plain_uri_against_tls_port_fails_or_times_out() {
    let container = common::start_broker_with_tls().await;
    let host = container.get_host().await.unwrap();
    let tls_port = container.get_host_port_ipv4(5671).await.unwrap();

    let uri = format!("amqp://{}:{}/{VHOST}", host, tls_port); // OJO: amqp, no amqps

    let result = tokio::time::timeout(
        Duration::from_secs(10),
        Connection::connect(&uri, ConnectionProperties::default()),
    )
    .await;

    // O bien el timeout salta (el broker nunca completa el handshake
    // AMQP porque esperaba TLS), o bien `lapin` devuelve un Err antes de
    // eso -- ambos son "rechazado". Lo único que NO puede pasar es
    // `Ok(Ok(_))` (conexión AMQP en claro completada con éxito).
    match result {
        Err(_elapsed) => {}         // timeout: tampoco completó el handshake
        Ok(Err(_lapin_err)) => {}   // lapin reportó el fallo explícitamente
        Ok(Ok(_conn)) => panic!("una conexión AMQP en claro NO debería completarse contra el puerto TLS"),
    }
}
```

**Recomendación:** usar §5.1 (TCP crudo) como el test que realmente cierra
la feature `tls` en `feature_list.json` (criterio de aceptación "una
conexión sin TLS al puerto 5671 es rechazada"); §5.2 puede añadirse como
test complementario si el implementer quiere reforzar la evidencia desde
el punto de vista de un cliente real (`lapin`), pero no debería ser la
única prueba porque su desenlace exacto (error vs. timeout) no está
garantizado por ningún contrato documentado de `lapin`.

---

## 6. Resumen de cambios necesarios (para el implementer, no aplicados aquí)

- **`Cargo.toml`**: ningún cambio de features de `lapin` (ver §1). Si se
  usa `tokio::time::timeout` en los tests (§5), la feature `time` de
  `tokio` ya está en el `Cargo.toml` actual del repo
  (`tokio = { version = "1.53.1", features = ["rt-multi-thread", "macros", "time"] }`).
- **`tests/common/mod.rs`**: añadir algo como `start_broker_with_tls()`
  (mismo patrón que `start_broker()` + los `with_mount` de §3.1) y
  `amqps_uri()`/`connect_amqps_lab()` (§2.5, §3.3) — o adaptar las
  funciones existentes si se decide que TLS es ya el único modo (a
  decidir junto con `explore_rabbitmq_tls.md`, que cubre si 5672 en claro
  sigue expuesto o no).
- **Certs de laboratorio**: dónde viven en el repo (`rabbitmq/tls/*.pem`
  o similar), cómo se generan (script `openssl`) y qué monta
  `rabbitmq.conf` (`ssl_options.cacertfile`/`certfile`/`keyfile`) es
  responsabilidad de `explore_rabbitmq_tls.md` — aquí solo se documentó
  el mecanismo de montaje (`Mount::bind_mount`, idéntico sea cual sea la
  ruta final) y el hecho de que el puerto no necesita configuración aparte
  en `testcontainers`.
- **Nuevo archivo de test** (p. ej. `tests/tls.rs`): con los dos tests de
  §4 y §5.1 (+ opcionalmente §5.2), siguiendo el patrón
  `#[ignore = "requiere Docker"]` + `mod common;` ya usado en
  `tests/topology_exists.rs`/`tests/permissions.rs`.

## 7. Fuentes citadas

- `lapin` v4.11.0 — `Cargo.toml`, `src/lib.rs`, `src/connection.rs`,
  `src/runtime.rs`, `src/generated/channel.rs`:
  <https://github.com/amqp-rs/lapin/tree/v4.11.0>
- `amq-protocol` v10.6.3 — `protocol/Cargo.toml`, `protocol/src/lib.rs`,
  `protocol/src/generated.rs`, `protocol/src/frame/generation.rs`,
  `tcp/Cargo.toml`, `tcp/src/lib.rs`:
  <https://github.com/amqp-rs/amq-protocol/tree/v10.6.3>
- `tcp-stream` v0.34.14 — `src/lib.rs`, `src/rustls_impl.rs`:
  <https://github.com/amqp-rs/tcp-stream/tree/v0.34.14>
- `rustls-connector` v0.23.8 — `src/lib.rs`:
  <https://github.com/amqp-rs/rustls-connector/tree/v0.23.8>
- `testcontainers` 0.27.0 — `testcontainers/src/runners/async_runner.rs`,
  `testcontainers/tests/host_port_exposure.rs`, y `ImageExt`/`Image`
  (docs.rs, cruzado con el código fuente para la parte de publicación de
  puertos): <https://github.com/testcontainers/testcontainers-rs/tree/0.27.0>
  · <https://docs.rs/testcontainers/0.27.0/testcontainers/core/trait.ImageExt.html>
- `testcontainers-modules` v0.15.0 — `src/rabbitmq/mod.rs`:
  <https://github.com/testcontainers/testcontainers-rs-modules-community/blob/v0.15.0/src/rabbitmq/mod.rs>
- `docker-library/rabbitmq` (rama `master`) — `4.3/ubuntu/Dockerfile`,
  `4.3/ubuntu/management/Dockerfile` (líneas `EXPOSE`):
  <https://github.com/docker-library/rabbitmq/tree/master/4.3/ubuntu>
