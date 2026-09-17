# Implementación: feature `tls` (id 3)

> Implementer, sesión 2026-09-17. Basado en `progress/explore_rabbitmq_tls.md`
> y `progress/explore_lapin_tls.md` (ya investigados por el leader, no se
> reabrió investigación). Referencia obligatoria leída antes de tocar nada:
> `AGENTS.md`, `docs/architecture.md`, `docs/conventions.md`,
> `docs/verification.md`, `docs/security-scope.md`.

## Archivos creados

- `rabbitmq/generate-lab-certs.sh` — script `openssl` que genera una CA de
  laboratorio propia (`O=security-app-lab`, `CN=security-app-lab-CA`) y un
  certificado de servidor firmado por ella (`CN=rabbitmq`, SAN
  `DNS:rabbitmq,DNS:localhost,IP:127.0.0.1`), salida en `rabbitmq/tls/`.
  Basado en la propuesta de `progress/explore_rabbitmq_tls.md` §2, con un
  ajuste no previsto por la investigación (ver "Desviaciones" abajo):
  `server_key.pem` se deja en modo `644` (no `640`), documentado en el
  propio script.
- `rabbitmq/tls/{ca_certificate,ca_key,server_certificate,server_key}.pem`
  — generados ejecutando el script. **No se commitean**: ya estaban
  cubiertos por `.gitignore` (`*.pem`, `*.key`, desde la feature
  `scaffolding`), así que se mantuvo esa decisión existente en vez de
  reabrirla — quien clone el repo debe correr
  `./rabbitmq/generate-lab-certs.sh` una vez antes de `docker compose up`
  (documentado en `rabbitmq/README.md`).
- `tests/tls.rs` — 2 tests `#[ignore = "requiere Docker"]`:
  - `amqps_connection_with_lab_ca_operates_on_real_topology`: conecta por
    AMQPS como `lab-admin` y publica en `scan.requests`, luego conecta por
    AMQPS como `ms-nmap` y confirma que el mensaje llega íntegro a
    `ms-nmap.scan-requests` (`basic_get`, con polling+timeout de 5s) —
    prueba la topología real de la feature 2 de punta a punta sobre TLS,
    no solo "la conexión no dio error".
  - `plaintext_connection_to_tls_port_is_rejected`: `tokio::net::TcpStream`
    crudo contra el puerto 5671, escribe el protocol header AMQP 0-9-1 en
    claro y confirma que el servidor no completa ningún handshake AMQP
    (timeout, EOF, reset, o — si responde algo — que esos bytes son un
    registro TLS, p. ej. una alerta fatal, nunca un frame AMQP real).

## Archivos modificados

- `rabbitmq/rabbitmq.conf` — añadido, bajo la línea `load_definitions`
  existente (sin tocarla): `listeners.ssl.default = 5671` +
  `ssl_options.cacertfile/certfile/keyfile` apuntando a
  `/etc/rabbitmq/tls/*.pem`. Sin `ssl_options.verify`/
  `fail_if_no_peer_cert` (TLS de transporte, no mTLS — según brief y
  `docs/security-scope.md`).
- `docker-compose.yml` — puerto `"5671:5671"` + volumen
  `./rabbitmq/tls:/etc/rabbitmq/tls:ro`; comentario de cabecera actualizado
  para explicar que 5672 es solo-dev y AMQPS es la vía real desde esta
  feature.
- `rabbitmq/README.md` — nueva sección "TLS (AMQPS, feature `tls`)":
  paso de setup (`generate-lab-certs.sh`), tabla de archivos generados,
  advertencia "NO usar en producción", configuración del listener,
  justificación de mantener 5672 abierto solo-dev, y la sección "Qué
  cambia para certificados reales de producción" (mismas claves de
  `rabbitmq.conf`, solo cambia el origen de los archivos — pedida
  explícitamente como criterio de aceptación).
- `Cargo.toml`:
  - `tokio` gana las features `net`/`io-util` (necesarias para
    `tokio::net::TcpStream` + `AsyncReadExt`/`AsyncWriteExt` en el test
    negativo). No se tocó ninguna feature de `lapin` (según el brief).
  - `[dev-dependencies] rustls = "0.23"` — necesaria para instalar
    explícitamente el `CryptoProvider` antes de la primera conexión AMQPS
    (ver "Desviaciones" abajo, bug no cubierto por la investigación
    previa).
- `tests/common/mod.rs`:
  - `start_broker()` (usada por los 10 tests ya existentes de la feature
    `topology_definition`) ahora también monta `rabbitmq/tls/*.pem` en
    `/etc/rabbitmq/tls/`. Fue un cambio **obligatorio, no opcional**: desde
    que `rabbitmq.conf` declara `ssl_options.*` de forma incondicional, el
    nodo RabbitMQ no arranca en absoluto (ni siquiera el listener 5672 en
    claro) si esos archivos no existen — sin este cambio los 10 tests
    existentes quedaban rotos por la sola presencia del bloque TLS en
    `rabbitmq.conf`, aunque ninguno de ellos use AMQPS. El comportamiento
    externo de `start_broker()` (misma topología, mismo puerto 5672 en
    claro para quien la llama) no cambió — solo lo que hace falta montar
    para que el contenedor arranque.
  - `start_broker_with_tls()` — alias de `start_broker()` (ver punto
    anterior: desde esta feature arrancar con la topología real siempre
    incluye el material TLS), usado por `tests/tls.rs` para dejar explícito
    en el sitio de uso que ese test depende de AMQPS.
  - Nuevas funciones: `tls_dir()`, `amqps_port()`, `amqps_uri()`,
    `connect_amqps_as()`, `ensure_rustls_crypto_provider()` (ver
    "Desviaciones").
  - Doc-comment de `amqp_uri()` actualizado (ya no dice "TLS pendiente").
- `progress/current.md` — bitácora actualizada en tiempo real durante la
  sesión (ver ese archivo para el detalle paso a paso).

## Desviaciones del brief (y por qué)

1. **`connect_amqps_as` colgaba indefinidamente** (no daba error) al
   primer intento de conexión AMQPS real. Diagnóstico: este crate trae, de
   forma transitiva, **dos proveedores criptográficos de `rustls`
   compilados a la vez** — `ring` (vía `reqwest`/`rustls-platform-verifier`,
   usado por la API HTTP de management) y `aws_lc_rs` (vía
   `testcontainers`/`bollard`/`hyper-rustls`, y también el que activa por
   defecto la propia feature `rustls` de `lapin`). Cuando hay más de un
   proveedor compilado, `rustls` no puede autoseleccionar uno: el hilo
   interno `lapin-io-loop` hacía panic con *"Could not automatically
   determine the process-level CryptoProvider"* — pero ese panic ocurre en
   un hilo separado, así que el `.await` de la conexión AMQPS nunca recibía
   ni éxito ni error: parecía un cuelgue, no un fallo. Esto no lo cubrió
   ninguno de los dos informes de investigación (ninguno anticipó que
   `reqwest`+`testcontainers` traen proveedores distintos). Solución:
   `ensure_rustls_crypto_provider()` en `tests/common/mod.rs` (guardada con
   `std::sync::Once`) instala explícitamente `aws_lc_rs` antes de la
   primera conexión AMQPS — requiere el nuevo `[dev-dependencies] rustls =
   "0.23"` en `Cargo.toml`. No se tocó ninguna feature de `lapin`/`reqwest`/
   `testcontainers`.
2. **Permisos de archivo del script de certs**: la propuesta original de
   `explore_rabbitmq_tls.md` (`chmod 640 ca_key.pem server_key.pem`) rompía
   el arranque de RabbitMQ (`ssl_options.keyfile invalid, PEM file does not
   exist, cannot be read...`). Causa: el contenedor oficial ejecuta
   RabbitMQ como el usuario no-root `rabbitmq` (vía `gosu`, confirmado
   leyendo `docker-entrypoint.sh` de la imagen), con un UID que no coincide
   con el UID del host que generó el archivo vía bind-mount — con `640`
   (solo dueño+grupo), ese usuario no podía leer la clave privada del
   servidor. Ajustado a `chmod 644 server_key.pem` (documentado en el
   propio script como necesario y aceptable solo para material de
   laboratorio, nunca real). `ca_key.pem` no se monta en ningún contenedor
   (no hace falta en runtime), así que se dejó en `640`.
3. **Test positivo — declaración pasiva rechazada por permisos**: el
   esqueleto propuesto en `explore_lapin_tls.md` §4 usaba
   `ms-nmap.exchange_declare("scan.requests", passive: true)`, asumiendo
   que una declaración pasiva no requiere `configure`. Contra RabbitMQ
   4.3.5 real, sí lo requiere (`403 ACCESS_REFUSED - configure access to
   exchange 'scan.requests' ... refused for user 'ms-nmap'` — `ms-nmap`
   tiene `configure: "^$"` en `rabbitmq/definitions.json`, coherente con
   mínimo privilegio). Se reemplazó por un flujo end-to-end real dentro de
   los permisos que `ms-nmap` sí tiene (`write` en `scan.outcomes`, `read`
   en `ms-nmap.scan-requests`): el admin publica por AMQPS en
   `scan.requests`, `ms-nmap` lo lee por AMQPS desde
   `ms-nmap.scan-requests` — sigue demostrando que la conexión TLS opera
   sobre la topología real (criterio de aceptación de la feature), y de
   paso es un test más fuerte (extremo a extremo, no solo introspección).
4. **Test negativo — falso positivo inicial**: la primera versión fallaba
   porque el servidor sí respondió bytes al *protocol header* AMQP en
   claro — pero esos bytes eran `[0x15, 0x03, 0x01, 0x00, 0x02, 0x02,
   0x0a]`, un **registro TLS de alerta fatal** (`ContentType=21 alert`,
   nivel `fatal`, descripción `unexpected_message`), exactamente la
   respuesta correcta de un servidor TLS al recibir texto plano — no AMQP
   en claro. El esqueleto de `explore_lapin_tls.md` §5.1 trataba *cualquier*
   byte recibido como fallo; se corrigió para distinguir: bytes que
   empiezan con un `ContentType` TLS válido (20-23) = rechazo correcto
   (test pasa); solo un frame que empiece con el byte de tipo `method`
   (`0x01`) de un `Connection.Start` real haría fallar el test.

## Resultado de `./init.sh`

Verde de punta a punta, ejecutado dos veces tras las correcciones:
`fmt --check`, `clippy --all-targets -- -D warnings`, `cargo test` (0
ignorados marcados correctamente), `cargo test -- --ignored` (**12/12**
tests de integración contra Docker real: los 10 de la feature
`topology_definition` sin cambios de comportamiento + los 2 nuevos de
`tls.rs`), `cargo doc --no-deps`. Log completo verificado manualmente
línea por línea (no solo el código de salida).

Verificación adicional manual (no exigida por `init.sh` pero relevante para
la feature): `docker compose up -d` real, `openssl s_client -connect
localhost:5671 -CAfile rabbitmq/tls/ca_certificate.pem` → `Verification:
OK`, `Protocol: TLSv1.3`; puerto 5672 confirmado igualmente accesible
(solo-dev); `docker compose down -v` sin dejar contenedores huérfanos
(`docker ps -a --filter ancestor=rabbitmq:4.3.5-management` vacío al
cerrar).

## Seguridad

- Ningún certificado/clave de producción: `CN=rabbitmq`/`security-app-lab`
  identifican el material como laboratorio a simple vista.
- Ninguna clave privada se loggeó ni se incluyó en mensajes de test/panic
  (los `panic!`/`expect` de los tests solo referencian rutas de archivo o
  errores de `lapin`, nunca el contenido de `server_key.pem`).
- `.pem`/`.key` siguen fuera de git (decisión ya existente de la feature
  `scaffolding`, confirmada con `git check-ignore -v`).

## Pendiente / bloqueos

Ninguno. La feature queda lista para revisión por el `reviewer`. No se
marcó `done` en `feature_list.json` (queda en `in_progress`, tal como
exige el protocolo del líder).

## Cierre de sesión — 2026-09-17

- **Veredicto del reviewer:** APPROVED, sin cambios requeridos
  (`progress/review_tls.md`). Única observación no bloqueante: `server_key.pem`
  usa `chmod 644` en vez de `640`, justificada en este mismo archivo (§
  "Desviaciones del brief", punto 2) y aceptada por tratarse de material de
  laboratorio desechable.
- **`./init.sh` re-ejecutado** una vez más antes del cierre (incluye
  `--ignored`, Docker disponible): verde de punta a punta, **12/12** tests de
  integración (los 10 de `topology_definition` sin cambio de comportamiento +
  los 2 nuevos de `tls.rs`).
- **`feature_list.json`**: id 3 (`tls`) → `status: "done"`.
- **`progress/current.md`**: resumen movido al final de `progress/history.md`
  (entrada "2026-09-17 — Feature 3 `tls` — DONE") y vaciado de vuelta a la
  plantilla original.
- **Docker**: `docker ps -a` y `docker compose ps -a` no muestran ningún
  contenedor ni volumen huérfano de este repo (los contenedores presentes en
  el host pertenecen a otros proyectos ajenos, ya existentes antes de esta
  sesión). No se levantó ningún stack adicional durante el cierre — solo se
  corrió el harness de `./init.sh`, que limpia sus propios `testcontainers`.
- **Sin archivos temporales sueltos**: `git status --porcelain=v1 -uall`
  confirma que los únicos archivos sin trackear son los entregables legítimos
  de esta feature (`progress/explore_*.md`, `progress/impl_tls.md`,
  `progress/review_tls.md`, `rabbitmq/generate-lab-certs.sh`, `tests/tls.rs`);
  `rabbitmq/tls/*.pem` generados localmente siguen fuera de git vía
  `.gitignore` (no aparecen en `git status -uall`), tal como confirmó el
  reviewer.
- **Commit**: pendiente — no se creó ningún commit en esta sesión de cierre
  (fuera del alcance de esta tarea; el líder decide cuándo commitear).
