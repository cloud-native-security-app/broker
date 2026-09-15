//! Helpers compartidos por los tests de integración de `topology_definition`.
//!
//! Sin `#[test]` propios — solo utilidades para arrancar un contenedor
//! `rabbitmq:management` real (vía `testcontainers`) con la topología de
//! `rabbitmq/definitions.json` cargada, y para conectar como cada usuario
//! de laboratorio. Ver `docs/verification.md` (Nivel 3): nunca mocks,
//! siempre contra un broker real y desechable.

use std::path::PathBuf;

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

/// Arranca un contenedor `rabbitmq:4.3.5-management` real con
/// `rabbitmq/definitions.json` y `rabbitmq/rabbitmq.conf` del repo
/// montados — la misma topología que carga `docker-compose.yml`.
pub async fn start_broker() -> ContainerAsync<RabbitMq> {
    let definitions_path = repo_root().join("rabbitmq/definitions.json");
    let conf_path = repo_root().join("rabbitmq/rabbitmq.conf");

    assert!(
        definitions_path.is_file(),
        "rabbitmq/definitions.json debe existir: {definitions_path:?}"
    );
    assert!(
        conf_path.is_file(),
        "rabbitmq/rabbitmq.conf debe existir: {conf_path:?}"
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
        .start()
        .await
        .expect("el contenedor RabbitMQ con la topología real debe arrancar")
}

/// Construye la URI AMQP (sin TLS — ver feature `tls`, todavía pendiente)
/// para conectar como `user` al vhost `security-app`.
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
