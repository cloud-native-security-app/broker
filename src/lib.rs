//! Crate de verificación de la topología y el contrato de mensajes del
//! Broker. No es una librería compartida ni produce un binario de
//! producción — ver `docs/architecture.md`.
//!
//! Desde la feature `topology_definition` este crate expone un único
//! módulo público, [`lab_credentials`], con las credenciales de
//! laboratorio (nunca reales) usadas tanto para calcular los
//! `password_hash` de `rabbitmq/definitions.json` como para que los tests
//! de integración de `tests/` se conecten como cada usuario. `definitions.json`
//! solo puede contener el *hash* de la contraseña (RabbitMQ no expone un
//! mecanismo confirmado de importar contraseñas en texto plano — ver
//! `progress/explore_definitions_format.md` §7), así que el valor en claro
//! tiene que vivir en un único lugar del lado Rust en vez de en el propio
//! `definitions.json`; ese lugar es este módulo, también documentado en
//! `rabbitmq/README.md` para quien no lea Rust.

/// Vhost dedicado del Broker (ver `rabbitmq/definitions.json`).
pub const VHOST: &str = "security-app";

/// Credenciales de RabbitMQ usadas únicamente en este entorno de
/// laboratorio/tests (`docker-compose.yml` local y los contenedores
/// efímeros de `testcontainers`). **Nunca** son válidas para un entorno de
/// producción — ver `docs/security-scope.md`.
///
/// Cada constante de contraseña coincide, byte a byte, con el valor a
/// partir del cual se calculó el `password_hash` correspondiente en
/// `rabbitmq/definitions.json` (algoritmo SHA-256 + salt de 4 bytes
/// documentado en <https://www.rabbitmq.com/docs/passwords>). Si se
/// cambia una contraseña aquí, hay que recalcular su hash y actualizar
/// `definitions.json` a la vez — no están sincronizados automáticamente.
pub mod lab_credentials {
    /// Usuario administrador (UI/API de management). Solo se usa en los
    /// tests para *verificar* la topología (nunca para operar como si
    /// fuera un servicio) — ver `docs/security-scope.md`.
    pub const ADMIN_USER: &str = "lab-admin";
    pub const ADMIN_PASSWORD: &str = "lab-only-not-a-real-secret";

    /// Usuario del servicio Gateway: publica en `scan.requests`, consume
    /// `gateway.scan-outcomes`.
    pub const GATEWAY_USER: &str = "gateway";
    pub const GATEWAY_PASSWORD: &str = "lab-only-not-a-real-secret-gateway";

    /// Usuario del servicio `ms-nmap`: consume `ms-nmap.scan-requests`,
    /// publica en `scan.outcomes`.
    pub const MS_NMAP_USER: &str = "ms-nmap";
    pub const MS_NMAP_PASSWORD: &str = "lab-only-not-a-real-secret-ms-nmap";

    /// Usuario del servicio `ms-analisis`: consume solo
    /// `ms-analisis.scan-outcomes`.
    pub const MS_ANALISIS_USER: &str = "ms-analisis";
    pub const MS_ANALISIS_PASSWORD: &str = "lab-only-not-a-real-secret-ms-analisis";
}
