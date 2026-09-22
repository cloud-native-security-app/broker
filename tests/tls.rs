//! Verifica AMQPS (TLS, puerto 5671) — feature `tls`.
//!
//! Dos tests: (1) una conexión AMQPS con la CA de laboratorio conecta y
//! opera sobre la topología real (feature `topology_definition`), y (2)
//! una conexión sin TLS al puerto AMQPS es rechazada — ver
//! `docs/verification.md` (Nivel 3) y `progress/explore_lapin_tls.md`.

mod common;

use std::time::Duration;

use broker_verification::lab_credentials as creds;
use lapin::options::{BasicGetOptions, BasicPublishOptions};
use lapin::BasicProperties;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const LAB_PAYLOAD: &[u8] = br#"{"note":"lab-only-not-a-real-secret tls payload"}"#;

/// Bytes exactos del *protocol header* AMQP 0-9-1 que envía cualquier
/// cliente (incluido `lapin`) nada más abrir la conexión — ver
/// `amq-protocol` v10.6.3 `protocol/src/generated.rs` +
/// `protocol/src/frame/generation.rs` (citado en
/// `progress/explore_lapin_tls.md` §5.1).
const AMQP_0_9_1_PROTOCOL_HEADER: &[u8] = b"AMQP\x00\x00\x09\x01";

/// Primer octeto de cualquier registro TLS (`ContentType`, RFC 8446 §5.1):
/// 20 = change_cipher_spec, 21 = alert, 22 = handshake, 23 = application_data.
/// Si el servidor responde con uno de estos, está hablando TLS (y
/// rechazando nuestro *protocol header* AMQP en claro con una alerta fatal,
/// típicamente `unexpected_message`) — evidencia correcta de que el puerto
/// exige TLS, no un fallo de la feature.
const TLS_CONTENT_TYPES: [u8; 4] = [20, 21, 22, 23];

#[tokio::test]
#[ignore = "requiere Docker"]
async fn amqps_connection_with_lab_ca_operates_on_real_topology() {
    let container = common::start_broker_with_tls().await;

    // El admin publica por AMQPS en scan.requests (exchange real de la
    // feature `topology_definition`) ...
    let admin = common::connect_amqps_as(&container, creds::ADMIN_USER, creds::ADMIN_PASSWORD)
        .await
        .unwrap_or_else(|err| {
            panic!("la conexion AMQPS del admin con la CA de laboratorio debe funcionar: {err}")
        });
    let admin_channel = admin.create_channel().await.expect("canal de admin");
    admin_channel
        .basic_publish(
            "scan.requests".into(),
            "scan.request".into(),
            BasicPublishOptions::default(),
            LAB_PAYLOAD,
            BasicProperties::default(),
        )
        .await
        .expect("publicar en scan.requests por AMQPS debe aceptarse")
        .await
        .expect("el broker debe confirmar la entrega por AMQPS");

    // ... y ms-nmap lo lee por AMQPS desde su propia cola real
    // (ms-nmap.scan-requests, bindeada a scan.requests en
    // rabbitmq/definitions.json): confirma que la conexión TLS realmente
    // opera sobre la topología real, no solo que "no dio error".
    let ms_nmap =
        common::connect_amqps_as(&container, creds::MS_NMAP_USER, creds::MS_NMAP_PASSWORD)
            .await
            .unwrap_or_else(|err| {
                panic!(
                    "la conexion AMQPS de ms-nmap con la CA de laboratorio debe funcionar: {err}"
                )
            });
    let ms_nmap_channel = ms_nmap.create_channel().await.expect("canal de ms-nmap");

    let received = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(message) = ms_nmap_channel
                .basic_get("ms-nmap.scan-requests".into(), BasicGetOptions::default())
                .await
                .expect("basic.get sobre AMQPS no debe fallar para ms-nmap en su propia cola")
            {
                return message.delivery.data;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("ms-nmap debe recibir por AMQPS el mensaje publicado en scan.requests");

    assert_eq!(received, LAB_PAYLOAD);
}

#[tokio::test]
#[ignore = "requiere Docker"]
async fn plaintext_connection_to_tls_port_is_rejected() {
    let container = common::start_broker_with_tls().await;
    let host = container.get_host().await.expect("host del contenedor");
    let tls_port = common::amqps_port(&container).await;

    let mut socket = TcpStream::connect((host.to_string().as_str(), tls_port))
        .await
        .expect("el socket TCP crudo si debe poder abrirse (5671 escucha)");

    // Si esto falla al escribir (broken pipe / connection reset), ya es
    // evidencia de rechazo -- se tolera como caso valido, no se hace
    // `expect` aqui.
    let write_result = socket.write_all(AMQP_0_9_1_PROTOCOL_HEADER).await;

    let mut buf = [0u8; 32];
    let read_result = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buf)).await;

    match (write_result, read_result) {
        // El write fallo (conexion ya cerrada por el servidor al ver
        // texto plano donde esperaba un TLS ClientHello).
        (Err(_), _) => {}
        // El write funciono pero el read se agoto -> el servidor no
        // completo ningun handshake AMQP (se quedo esperando TLS y nunca
        // respondio) -- tambien cuenta como rechazo del handshake AMQP en
        // claro.
        (Ok(_), Err(_timeout)) => {}
        // El read termino: Ok(0) (EOF -- el servidor cerro el socket) o un
        // Err de IO (reset) -- ambos son el resultado esperado.
        (Ok(_), Ok(Ok(0))) => {}
        (Ok(_), Ok(Err(_io_err))) => {}
        // Llegaron bytes: solo es un rechazo valido si el primer octeto es
        // un ContentType de un registro TLS (el servidor esta hablando
        // TLS -- normalmente una alerta fatal como "unexpected_message" al
        // recibir texto plano donde esperaba un ClientHello). Si en cambio
        // parece un frame AMQP (arranca con el octeto de tipo `method`,
        // 0x01, de un Connection.Start real), el puerto SI hablo AMQP en
        // claro -- eso es un fallo real de la feature.
        (Ok(_), Ok(Ok(n))) => {
            assert!(
                TLS_CONTENT_TYPES.contains(&buf[0]),
                "el puerto 5671 respondio {n} bytes que no son un registro TLS (se esperaba un \
                 ContentType 20-23, p. ej. una alerta TLS fatal) -- posible AMQP en claro: {:?}",
                &buf[..n]
            );
        }
    }
}
