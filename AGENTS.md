# Reglas de revisión de código — broker

> Reglas que usa el revisor automático (Gentleman Guardian Angel) sobre cada
> commit. Un diff que viole una regla marcada como **bloqueante** se rechaza.

## Contexto

`broker` define el contrato de mensajería de la plataforma: los contratos de
mensajes en JSON Schema (`contracts/`), la topología SNS + SQS esperada como
datos puros (`src/topology.rs`) y un crate Rust (`src/`, `tests/`) que los
verifica contra LocalStack. La topología se implementa en el repo `infra`
(Terraform y init de LocalStack). No tiene binario de producción. Por
`scan-requests` viajan **credenciales SSH reales**
(`ScanRequest.ssh_credentials_ref`); ver `docs/security-scope.md`. RabbitMQ
fue retirado (`docs/decisions.md`, D-13).

## Seguridad (bloqueante)

- Nunca se commitean credenciales, certificados ni claves privadas reales.
  Los payloads de ejemplo y las credenciales de LocalStack (`test`/`test`) son
  de laboratorio y deben verse ficticios a simple vista (p. ej.
  `lab-only-not-a-real-secret`).
- El HTTP en claro solo se acepta contra el LocalStack efímero de los tests;
  contra AWS real siempre HTTPS, y las políticas de cola/tópico niegan
  `aws:SecureTransport=false` (comprobado por paridad con `infra`).
- Ningún test, script ni config loguea el cuerpo completo de un mensaje que
  pueda contener `ssh_credentials_ref` (en aserciones se compara con `assert!`
  sin imprimir el cuerpo).
- Retención acotada: la cola de requests conserva 1 h y su DLQ 24 h, con
  SSE-SQS; nada que retenga mensajes con credenciales indefinidamente.

## Topología y contratos (bloqueante)

- `src/topology.rs` es la única fuente de verdad de nombres y parámetros
  (tópicos, colas, `-dlq`, visibilidad, retención, filtros,
  `maxReceiveCount`). Tests y docs que citan un nombre deben coincidir
  exactamente; `tests/topology_parity.rs` lo compara con `infra`.
- Nombres según `docs/conventions.md`: tópicos `<dominio>-<plural>`, colas
  `<servicio-consumidor>-<qué-consume>`, DLQ `<cola>-dlq`, `routing_key` como
  atributo que describe el evento y no el destinatario.
- Toda cola de trabajo tiene su DLQ y `RedrivePolicy.maxReceiveCount`
  declarados en la topología, nunca implícitos en el código de los tests.
- Un cambio en `contracts/*.schema.json` refleja literalmente la forma que ya
  implementa el productor/consumidor (`ms-nmap` `domain.rs`). Un cambio
  incompatible del contrato es una decisión explícita, documentada en
  `contracts/README.md` y `docs/decisions.md`, no un ajuste silencioso.
- No se crea un tópico nuevo solo porque un consumidor quiere un subconjunto
  de eventos existentes: se usa otra suscripción con `FilterPolicy`.

## Corrección (bloqueante)

- Nada de `unwrap()`/`expect()`/`panic!()` fuera de tests.
- Errores tipados con `thiserror`; nada de `String` como tipo de error.
- Ninguna llamada bloqueante dentro de código async (`tokio`).

## Tests

- Todo cambio de topología, retención, cifrado o contrato trae su test en el
  mismo commit.
- Los tests de integración corren contra LocalStack real vía `testcontainers`,
  marcados `#[ignore = "requiere Docker"]`; nunca mocks de SNS/SQS ni cuentas
  AWS reales.
- Los tests de enrutamiento cubren el caso positivo (llega a su cola) y el
  negativo (no llega a las ajenas ni pasa el filtro).
- Nombres descriptivos del comportamiento
  (`gateway_receives_all_three_outcomes_and_the_filter_drops_other_routing_keys`).

## Estilo

- `cargo fmt` y `cargo clippy --all-targets -- -D warnings` sin warnings
  (`make verify` corre ambos).
- Sin `println!`/`dbg!` de depuración ni TODOs sin contexto.
- Comentarios solo para explicar un *por qué* no obvio.
- Nombres: `snake_case` para módulos, funciones y variables; `PascalCase`
  para tipos; `UPPER_SNAKE` para constantes.
