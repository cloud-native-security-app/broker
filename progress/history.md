# Bitácora histórica (append-only)

> Cada vez que se cierra una sesión, su resumen se añade aquí.
> No edites entradas anteriores. Solo añades al final.

---

## 2026-09-14 — Feature 1 `scaffolding` — DONE

- **Feature completada:** id 1, `scaffolding` — esqueleto del repo antes de
  definir la topología real.
- **Qué se creó:** `docker-compose.yml` (servicio `rabbitmq` con imagen
  `rabbitmq:4.3.5-management` pineada, puertos `5672`/`15672`, usuario
  `lab-admin`/`lab-only-not-a-real-secret` documentado explícitamente como
  credencial de laboratorio); `rabbitmq/README.md` y `contracts/README.md`
  como placeholders de las features `topology_definition` y
  `message_contract`; `Cargo.toml` del crate `broker-verification`
  (`edition = "2021"`, `publish = false`, sin `[[bin]]`, sin dependencias) y
  `src/lib.rs` vacío con doc-comment; `Cargo.lock` generado por cargo.
  `.gitignore` ya cubría `/target`, `*.tmp`, `*.pem`, `*.key` desde antes, no
  requirió cambios.
- **Veredicto del reviewer:** APPROVED, sin cambios requeridos. Verificó por
  su cuenta `./init.sh` (verde), `docker compose config` (válido), y
  `docker manifest inspect rabbitmq:4.3.5-management` (tag real, no
  inventado). Contrastó los 7 criterios de aceptación de
  `feature_list.json` uno a uno — todos cumplidos. Detalle completo en
  `progress/review_scaffolding.md`.
- **Cierre de sesión:** `./init.sh` re-ejecutado en verde de punta a punta
  (secciones 1-6 OK, 0 tests válido para crate vacío). `feature_list.json`
  id 1 → `status: "done"`. Sin contenedores Docker huérfanos de esta sesión
  (`docker ps -a` no muestra ningún contenedor RabbitMQ/broker; los
  contenedores existentes pertenecen a otros proyectos y ya estaban
  detenidos antes de esta sesión). Sin archivos temporales sueltos.
- **Fecha:** 2026-09-14.

---

## 2026-09-14 — Feature 2 `topology_definition` — DONE

- **Feature completada:** id 2, `topology_definition` — topología real de
  RabbitMQ que conecta Gateway → ms-nmap, ms-nmap → ms-analisis y
  ms-nmap → Gateway (RF-08), con reintentos acotados (RNF-06) y usuarios de
  servicio de mínimo privilegio.
- **Qué se creó:**
  - `rabbitmq/definitions.json`: vhost `security-app`; exchanges `scan.requests`
    (topic) + `scan.requests.dlx` (direct) y `scan.outcomes` (topic) +
    `scan.outcomes.dlx` (direct); colas quorum `ms-nmap.scan-requests`
    (routing key `scan.request`), `ms-analisis.scan-outcomes` (solo
    `scan.outcome.completed`/`scan.outcome.failed`, sin `started`) y
    `gateway.scan-outcomes` (`scan.outcome.#`, las tres variantes), cada una
    con su `.dlq`; 4 usuarios (`lab-admin` + 3 de servicio: `gateway`,
    `ms-nmap`, `ms-analisis`), cada uno con permisos `configure/write/read`
    acotados por regex a exactamente lo que su servicio necesita (sin acceso
    cruzado entre colas/exchanges de otros servicios) y contraseñas de
    laboratorio (`lab-only-not-a-real-secret*`, nunca reales).
  - Patrón de reintentos acotados (RNF-06): `x-delivery-limit: 3` en las 3
    colas principales (colas quorum), en vez del patrón clásico
    "cola-retry+TTL" — decisión justificada y verificada empíricamente
    contra RabbitMQ 4.3.5 real (desviación documentada: hay que usar
    `basic.reject`, no `basic.nack`, para que `x-delivery-limit` cuente las
    redeliveries). N=3 documentado y justificado en `rabbitmq/README.md`.
  - `rabbitmq/rabbitmq.conf` (`load_definitions = ...`, sin prefijo
    `management.`, ya que `RABBITMQ_LOAD_DEFINITIONS` es de la imagen
    Bitnami, no la oficial) y `docker-compose.yml` actualizado para montar
    ambos archivos; gestión de `lab-admin` movida de env vars a
    `definitions.json` como fuente única de verdad de usuarios/permisos.
  - `Cargo.toml`/`src/lib.rs` con las dependencias necesarias
    (`testcontainers`, `testcontainers-modules` feature `rabbitmq`, `lapin`,
    `tokio`, `reqwest` feature `json`, `serde`/`serde_json`) y credenciales
    de laboratorio como constantes documentadas.
  - 10 tests de integración nuevos en `tests/` (`#[ignore = "requiere
    Docker"]`, contra `rabbitmq:4.3.5-management` real vía
    `testcontainers`+`lapin`+`reqwest`, nunca mocks): `topology_exists.rs`
    (exchanges/colas/bindings exactos, incluida la ausencia del binding
    `scan.outcome.started` en `ms-analisis.scan-outcomes`),
    `permissions.rs` (6 tests: caso positivo y negativo para los 3 usuarios
    de servicio, incluidos los sub-casos de acceso cruzado denegado),
    `outcome_routing.rs` (2 tests: `started` solo llega a
    `gateway.scan-outcomes`, `completed` llega a ambas colas),
    `retry_delivery_limit.rs` (1 test: un mensaje que agota
    `x-delivery-limit` termina en su `.dlq` con
    `x-death[].reason == "delivery_limit"`, confirmando que pasó por el
    ciclo de reintentos y no cayó directo a la `.dlq`).
- **Veredicto del reviewer:** APPROVED sin cambios bloqueantes
  (`progress/review_topology_definition.md`). Verificó por su cuenta
  `./init.sh` completo (fmt/clippy -D warnings/test/test --ignored/doc,
  10/10 tests verdes contra Docker real), levantó el stack con `docker
  compose up -d` y confirmó en logs reales que `load_definitions` carga
  correctamente 4 usuarios/vhost/4 exchanges/6 colas/7 bindings en el
  primer arranque, bajó el stack con `docker compose down -v` sin dejar
  huérfanos, y confirmó ausencia de credenciales reales/parecidas a reales
  vía `grep`. Única observación no bloqueante: la dependencia `futures =
  "0.3.34"` en `Cargo.toml` no se usaba en ningún archivo del crate.
- **Corrección post-revisión:** se eliminó `futures = "0.3.34"` de
  `Cargo.toml` (confirmado con `grep -rn "futures" tests/ src/ Cargo.toml`
  que solo aparecía la propia línea de la dependencia, sin uso real en
  ningún archivo); `Cargo.lock` se actualizó solo al recompilar. Se
  re-ejecutó `./init.sh` completo tras el cambio: verde de punta a punta,
  10/10 tests de integración (`--ignored`) siguen pasando contra Docker
  real.
- **Cierre de sesión:** `feature_list.json` id 2 → `status: "done"`. Sin
  contenedores/volúmenes Docker huérfanos de esta sesión (`docker ps -a` y
  `docker compose ps -a` no muestran ningún contenedor RabbitMQ de este
  repo; testcontainers se limpió solo tras los tests). Sin archivos
  temporales sueltos.
- **Fecha:** 2026-09-14.

---

## 2026-09-17 — Feature 3 `tls` — DONE

- **Feature completada:** id 3, `tls` — TLS (AMQPS) obligatorio para toda
  conexión al Broker, requisito duro porque el `ScanRequest` transporta una
  credencial SSH real en `ssh_credentials_ref` (`docs/security-scope.md`).
- **Qué se habilitó:**
  - Listener AMQPS en el puerto `5671` de RabbitMQ (`rabbitmq/rabbitmq.conf`:
    `listeners.ssl.default = 5671` + `ssl_options.cacertfile/certfile/keyfile`
    apuntando a `/etc/rabbitmq/tls/*.pem`), sin tocar la línea
    `load_definitions` ya existente.
  - `rabbitmq/generate-lab-certs.sh`: script `openssl` que genera una CA de
    laboratorio propia (`O=security-app-lab`, `CN=security-app-lab-CA`) y un
    certificado de servidor firmado por ella (`CN=rabbitmq`, SAN
    `DNS:rabbitmq,DNS:localhost,IP:127.0.0.1`), salida en `rabbitmq/tls/`
    (fuera de git, cubierto por `.gitignore` desde `scaffolding`). Documentado
    explícita y repetidamente como material NO apto para producción.
  - `docker-compose.yml`: puerto `"5671:5671"` expuesto + volumen
    `./rabbitmq/tls:/etc/rabbitmq/tls:ro`. **Decisión del usuario**: el puerto
    `5672` en claro se mantiene abierto, marcado explícitamente en el
    comentario de cabecera del compose y en `rabbitmq/README.md` como
    solo-desarrollo/depuración local — ningún servicio de producción debe
    apuntar a él.
  - `rabbitmq/README.md`: sección "TLS (AMQPS, feature `tls`)" con el paso de
    setup del script, advertencia de laboratorio, y la sección "Qué cambia
    para certificados reales de producción" (mismas claves de
    `rabbitmq.conf`, solo cambia el origen de los archivos; la clave privada
    real nunca se commitea, vía secret management de la plataforma).
  - 2 tests nuevos en `tests/tls.rs` (`#[ignore = "requiere Docker"]`, contra
    `rabbitmq:4.3.5-management` real vía `testcontainers`+`lapin`, nunca
    mocks): `amqps_connection_with_lab_ca_operates_on_real_topology`
    (conecta por AMQPS con la CA de laboratorio y opera de punta a punta
    sobre la topología real de la feature 2 — publica como `lab-admin`, lee
    como `ms-nmap` su propia cola) y
    `plaintext_connection_to_tls_port_is_rejected` (conexión TCP cruda en
    claro contra 5671 nunca completa un handshake AMQP).
  - `tests/common/mod.rs` ajustado para montar los `.pem` de TLS también en
    `start_broker()` (obligatorio: `rabbitmq.conf` declara `ssl_options.*`
    incondicionalmente y el nodo no arranca sin ellos), sin cambiar el
    comportamiento externo de los 10 tests previos.
  - `Cargo.toml`: `tokio` gana features `net`/`io-util`; `[dev-dependencies]
    rustls = "0.23"` para instalar explícitamente el `CryptoProvider`
    `aws_lc_rs` antes de la primera conexión AMQPS (desviación documentada en
    `progress/impl_tls.md`: el crate trae dos proveedores rustls
    transitivos — `ring` y `aws_lc_rs` — y sin esta fijación la conexión
    AMQPS colgaba indefinidamente en vez de fallar).
- **Veredicto del reviewer:** APPROVED, sin cambios requeridos
  (`progress/review_tls.md`). Verificó por su cuenta `./init.sh` completo
  (fmt/clippy -D warnings/test/test --ignored/doc, **12/12** tests de
  integración verdes contra Docker real: los 10 previos sin cambio de
  comportamiento + los 2 nuevos de `tls.rs`), confirmó cadena CA→servidor
  real (no autofirmado suelto), ausencia de credenciales/`.pem` reales
  commiteados, y que ninguna otra feature (`contracts/`, permisos de
  `definitions.json`) fue adelantada. Única observación no bloqueante:
  `server_key.pem` usa `chmod 644` en vez de `640` — justificado (el usuario
  `rabbitmq` del contenedor oficial no coincide con el UID del host que
  generó el archivo vía bind-mount) y aceptable por tratarse de material de
  laboratorio desechable, nunca real.
- **Cierre de sesión:** `./init.sh` re-ejecutado en verde de punta a punta
  (12/12 tests de integración contra Docker real). `feature_list.json` id 3
  → `status: "done"`. Sin contenedores/volúmenes Docker huérfanos de esta
  sesión (`docker ps -a` no muestra ningún contenedor RabbitMQ de este repo;
  los contenedores presentes pertenecen a otros proyectos ajenos, ya
  existentes antes de esta sesión). Sin archivos temporales sueltos.
- **Fecha:** 2026-09-17.

---

## 2026-09-17 — Feature 4 `message_contract` — DONE

- **Feature completada:** id 4, `message_contract` — contrato de mensajes
  (JSON Schema de `ScanRequest` y `ScanOutcome`, con las variantes
  `started`/`completed`/`failed`), congelado y documentado a partir del
  shape real ya implementado y testeado en `ms-nmap` (no re-derivado desde
  cero).
- **Qué se creó:**
  - `contracts/scan-request.schema.json` — JSON Schema draft 2020-12 de
    `ScanRequest`: 6 campos (`correlation_id`, `ip`, `network_user`,
    `ssh_credentials_ref`, `has_sudo`, `requested_by`), todos `required`,
    `additionalProperties: false`. `ip` validado con `pattern` (regex
    IPv4/IPv6) en vez de `format`, decisión justificada (el crate
    `jsonschema` no valida `format` por defecto en 2020-12).
  - `contracts/scan-outcome.schema.json` — JSON Schema con `oneOf` de 3
    variantes discriminadas por `status`: `started` (solo
    `correlation_id`, sin `result` ni `reason` — documentado como
    "contrato acordado, publicación PENDIENTE en `ms-nmap`", ya que el
    código real de `ms-nmap` solo tiene `Completed`/`Failed`),
    `completed` (con `result: $defs/scanResult`) y `failed` (con
    `reason`), cada una `additionalProperties: false`. `$defs/scanResult`
    incluye el shape completo de `PortFinding`/`VulnFinding` y los 4 enums
    (`Protocol`, `PortState`, `Severity`, `VulnSource`) con los valores
    string exactos de `ms-nmap`.
  - `contracts/README.md` — documenta qué exchange/routing-key transporta
    cada mensaje y quién publica/consume, contrastado contra
    `rabbitmq/definitions.json` real (incluida la diferencia de bindings
    entre `ms-analisis.scan-outcomes` y `gateway.scan-outcomes`); marca
    `started` explícitamente como pendiente en `ms-nmap`; nota explícita
    de ausencia de `schema_version` y política de cambios aditivos.
  - `src/contracts.rs` — helpers de validación (`validate_scan_request`,
    `validate_scan_outcome`) vía crate `jsonschema` (schemas embebidos con
    `include_str!`, compilados una vez por proceso), `ContractError` con
    `thiserror`; 11 tests unitarios sin Docker (payloads válidos y varios
    casos de rechazo: campo faltante, campo extra, `ip` inválida,
    `completed`/`failed` con campos cruzados, `status` desconocido).
  - `tests/message_contract.rs` — test de integración smoke end-to-end
    (`#[ignore = "requiere Docker"]`, contra `rabbitmq:4.3.5-management`
    real vía `testcontainers`+`lapin`): publica un `ScanRequest` válido y
    verifica que llega íntegro a `ms-nmap.scan-requests`; publica las 3
    variantes de `ScanOutcome` y verifica el enrutamiento exacto (las 3 a
    `gateway.scan-outcomes`, solo `completed`/`failed` a
    `ms-analisis.scan-outcomes`).
  - `Cargo.toml`/`Cargo.lock` — nuevas dependencias `jsonschema` (sin
    features de resolución remota) y `thiserror`.
- **Veredicto del reviewer:** APPROVED, sin cambios requeridos
  (`progress/review_message_contract.md`). El reviewer contrastó campo por
  campo `contracts/scan-request.schema.json` y
  `contracts/scan-outcome.schema.json` directamente contra el código
  fuente real de `ms-nmap` (`domain.rs`, `publisher.rs`), verificó
  `./init.sh` completo (11 unitarios + 14 de integración contra Docker
  real), y confirmó ausencia de credenciales reales y que ningún archivo
  fuera del alcance de la feature (`rabbitmq/definitions.json`, TLS) fue
  tocado. Dos observaciones menores no bloqueantes, sin acción requerida:
  redacción ambigua en la tabla de `contracts/README.md` línea 16 (el
  contenido técnico correcto ya está aclarado en el cuerpo del documento)
  y el uso justificado de `panic!()` en `src/contracts.rs::compile`
  (documentado como aceptable mientras solo compile los 2 schemas propios
  del repo).
- **Cierre de sesión:** `./init.sh` re-ejecutado en verde de punta a punta
  (11 tests unitarios + 14 tests de integración contra Docker real, todos
  pasando). `feature_list.json` id 4 → `status: "done"`. Sin
  contenedores/volúmenes Docker huérfanos de esta sesión (`docker ps -a`
  no muestra ningún contenedor de este repo — testcontainers se limpió
  solo tras los tests; `docker compose ps -a` no muestra servicios
  levantados). Sin archivos temporales sueltos.
- **Fecha:** 2026-09-17.
