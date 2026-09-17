//! Helpers compartidos por los tests de integración de `topology_definition`.
//!
//! Sin `#[test]` propios — solo utilidades para arrancar un contenedor
//! `rabbitmq:management` real (vía `testcontainers`) con la topología de
//! `rabbitmq/definitions.json` cargada, y para conectar como cada usuario
//! de laboratorio. Ver `docs/verification.md` (Nivel 3): nunca mocks,
//! siempre contra un broker real y desechable.

use std::path::PathBuf;

use lapin::tcp::OwnedTLSConfig;
use lapin::{Connection, ConnectionProperties};
use testcontainers::core::{ImageExt, Mount};
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::rabbitmq::RabbitMq;

use broker_verification::VHOST;

/// Tag de imagen idéntico al fijado en `docker-compose.yml`, para no
/// verificar contra una versión de RabbitMQ distinta de la de referencia
/// del repo.
pub const RABBITMQ_TAG: &str = "4.3.5-management";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Ruta al directorio `rabbitmq/tls/` del repo, con los certificados de
/// laboratorio generados por `rabbitmq/generate-lab-certs.sh` (ver
/// `rabbitmq/README.md`). No se genera aquí: quien corra estos tests debe
/// haber ejecutado el script antes (mismo requisito que
/// `docker-compose.yml`).
#[allow(dead_code)]
fn tls_dir() -> PathBuf {
    repo_root().join("rabbitmq/tls")
}

/// Arranca un contenedor `rabbitmq:4.3.5-management` real con
/// `rabbitmq/definitions.json`, `rabbitmq/rabbitmq.conf` y
/// `rabbitmq/tls/` (certificados de laboratorio) del repo montados — la
/// misma topología y el mismo material TLS que carga `docker-compose.yml`.
///
/// Desde la feature `tls`, `rabbitmq/rabbitmq.conf` declara
/// `ssl_options.cacertfile/certfile/keyfile` de forma incondicional: el
/// nodo RabbitMQ **no arranca en absoluto** (ni siquiera para servir el
/// listener AMQP en claro de 5672) si esos archivos no existen — por eso
/// esta función, usada tanto por los tests de la feature
/// `topology_definition` (que solo conectan por 5672 en claro) como por
/// `tests/tls.rs`, siempre monta los tres archivos `.pem` aunque el test
/// que la invoque no vaya a usar AMQPS.
#[allow(dead_code)]
pub async fn start_broker() -> ContainerAsync<RabbitMq> {
    let definitions_path = repo_root().join("rabbitmq/definitions.json");
    let conf_path = repo_root().join("rabbitmq/rabbitmq.conf");
    let tls_dir = tls_dir();

    assert!(
        definitions_path.is_file(),
        "rabbitmq/definitions.json debe existir: {definitions_path:?}"
    );
    assert!(
        conf_path.is_file(),
        "rabbitmq/rabbitmq.conf debe existir: {conf_path:?}"
    );
    assert!(
        tls_dir.is_dir(),
        "rabbitmq/tls/ debe existir — ejecuta ./rabbitmq/generate-lab-certs.sh antes de correr estos tests: {tls_dir:?}"
    );

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
            tls_dir
                .join("ca_certificate.pem")
                .to_string_lossy()
                .into_owned(),
            "/etc/rabbitmq/tls/ca_certificate.pem",
        ))
        .with_mount(Mount::bind_mount(
            tls_dir
                .join("server_certificate.pem")
                .to_string_lossy()
                .into_owned(),
            "/etc/rabbitmq/tls/server_certificate.pem",
        ))
        .with_mount(Mount::bind_mount(
            tls_dir
                .join("server_key.pem")
                .to_string_lossy()
                .into_owned(),
            "/etc/rabbitmq/tls/server_key.pem",
        ))
        .start()
        .await
        .expect("el contenedor RabbitMQ con la topología real (AMQP+AMQPS) debe arrancar")
}

/// Alias de [`start_broker`] para los tests de la feature `tls`
/// (`tests/tls.rs`): desde esta feature, arrancar el broker con la
/// topología real siempre incluye el material TLS (ver el doc-comment de
/// `start_broker`) — se mantiene este nombre porque deja explícito, en el
/// sitio donde se usa, que el test que sigue depende de AMQPS.
#[allow(dead_code)]
pub async fn start_broker_with_tls() -> ContainerAsync<RabbitMq> {
    start_broker().await
}

/// Puerto AMQPS (5671) mapeado en el host para `container`.
#[allow(dead_code)]
pub async fn amqps_port(container: &ContainerAsync<RabbitMq>) -> u16 {
    container
        .get_host_port_ipv4(5671)
        .await
        .expect("puerto AMQPS (5671) mapeado")
}

/// Construye la URI AMQPS (`amqps://...`) para conectar como `user` al
/// vhost `security-app`.
#[allow(dead_code)]
pub async fn amqps_uri(container: &ContainerAsync<RabbitMq>, user: &str, password: &str) -> String {
    let host = container.get_host().await.expect("host del contenedor");
    let port = amqps_port(container).await;
    format!("amqps://{user}:{password}@{host}:{port}/{VHOST}")
}

/// Instala explícitamente el `CryptoProvider` `aws_lc_rs` de `rustls` antes
/// de la primera conexión AMQPS del proceso de test.
///
/// Sin esto, `lapin`/`rustls` **no cuelgan un error**: la conexión AMQPS se
/// queda esperando para siempre. Motivo: este crate de tests trae, de forma
/// transitiva, dos proveedores criptográficos de `rustls` compilados a la
/// vez en el mismo binario — `ring` (vía `reqwest`/`rustls-platform-verifier`,
/// usado para la API HTTP de management) y `aws_lc_rs` (vía
/// `testcontainers`/`bollard`/`hyper-rustls`, y también el que activa por
/// defecto la feature `rustls` de `lapin`). Cuando hay más de un proveedor
/// compilado, `rustls` no puede auto-seleccionar uno: el hilo interno
/// `lapin-io-loop` hace panic con "Could not automatically determine the
/// process-level CryptoProvider" **en un hilo separado**, así que el
/// `.await` de la conexión AMQPS nunca recibe respuesta ni error — parece
/// un cuelgue, no un fallo. Instalarlo una sola vez por proceso (ver
/// <https://docs.rs/rustls/latest/rustls/crypto/struct.CryptoProvider.html>)
/// resuelve la ambigüedad sin tocar features de `lapin`/`reqwest`/
/// `testcontainers`.
#[allow(dead_code)]
fn ensure_rustls_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

/// Conecta por AMQPS como `user`, confiando en la CA de laboratorio
/// (`rabbitmq/tls/ca_certificate.pem`) además del verificador de
/// plataforma por defecto de `lapin`/`rustls` (ver
/// `progress/explore_lapin_tls.md` §2.3). Sin autenticación de cliente por
/// certificado (`identity: None`) — este repo no exige mTLS, ver
/// `docs/security-scope.md`.
#[allow(dead_code)]
pub async fn connect_amqps_as(
    container: &ContainerAsync<RabbitMq>,
    user: &str,
    password: &str,
) -> lapin::Result<Connection> {
    ensure_rustls_crypto_provider();
    let uri = amqps_uri(container, user, password).await;
    let ca_pem = std::fs::read_to_string(tls_dir().join("ca_certificate.pem"))
        .expect("la CA de laboratorio (rabbitmq/tls/ca_certificate.pem) debe existir");

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

/// Construye la URI AMQP en claro (puerto 5672) para conectar como `user`
/// al vhost `security-app`. Usada por `tests/topology_exists.rs`,
/// `tests/permissions.rs`, `tests/outcome_routing.rs` y
/// `tests/retry_delivery_limit.rs` (feature `topology_definition`), que
/// verifican la topología/permisos/contrato en sí, no el transporte — ver
/// `amqps_uri`/`connect_amqps_as` arriba para la vía TLS real (feature
/// `tls`, ver `docs/security-scope.md`: 5672 es solo desarrollo/depuración).
///
/// (Ver nota de `#[allow(dead_code)]` en `management_base_url` más abajo:
/// mismo motivo — no todos los binarios de test de este módulo compartido
/// abren conexiones AMQP como usuario de servicio.)
#[allow(dead_code)]
pub async fn amqp_uri(container: &ContainerAsync<RabbitMq>, user: &str, password: &str) -> String {
    let host = container.get_host().await.expect("host del contenedor");
    let port = container
        .get_host_port_ipv4(5672)
        .await
        .expect("puerto AMQP mapeado");
    format!("amqp://{user}:{password}@{host}:{port}/{VHOST}")
}

/// Conecta como `user` con sus credenciales de laboratorio
/// (`broker_verification::lab_credentials`). Falla el test si el usuario no
/// puede ni siquiera abrir la conexión (p. ej. contraseña incorrecta).
#[allow(dead_code)]
pub async fn connect_as(
    container: &ContainerAsync<RabbitMq>,
    user: &str,
    password: &str,
) -> Connection {
    let uri = amqp_uri(container, user, password).await;
    Connection::connect(&uri, ConnectionProperties::default())
        .await
        .unwrap_or_else(|err| {
            panic!("{user} debe poder conectar con sus credenciales de laboratorio: {err}")
        })
}

/// Base URL de la API HTTP de management del contenedor.
///
/// `tests/common/mod.rs` se recompila por cada binario de test que hace
/// `mod common;`; no todos usan la API HTTP de management (algunos solo
/// verifican por AMQP), así que esta función aparece como "no usada" en
/// esos binarios — falso positivo de `dead_code` esperado en un módulo de
/// helpers compartido, no un error real.
#[allow(dead_code)]
pub async fn management_base_url(container: &ContainerAsync<RabbitMq>) -> String {
    let host = container.get_host().await.expect("host del contenedor");
    let port = container
        .get_host_port_ipv4(15672)
        .await
        .expect("puerto de management mapeado");
    format!("http://{host}:{port}/api")
}
