# Review — feature `tls` (id 3)

**Veredicto:** APPROVED

## Verificación realizada

- Leídos: `docs/architecture.md`, `docs/conventions.md`, `docs/security-scope.md`,
  `docs/verification.md`, `CHECKPOINTS.md`, `feature_list.json` (criterios exactos
  de la feature 3), `progress/impl_tls.md`, `progress/current.md`.
- Inspeccionado el contenido real (no solo el informe) de: `rabbitmq/rabbitmq.conf`,
  `rabbitmq/generate-lab-certs.sh`, `docker-compose.yml`, `rabbitmq/README.md`,
  `tests/tls.rs`, `tests/common/mod.rs`, `Cargo.toml`, `.gitignore`, `src/lib.rs`,
  `contracts/README.md`, `feature_list.json` diff.
- Ejecutado `git status`/`git diff` para confirmar qué se modificó realmente y qué
  quedó fuera de git.
- Ejecutado `./init.sh` de punta a punta con Docker real disponible.

## Hallazgos por punto de la tarea

1. **`rabbitmq/rabbitmq.conf`** (líneas 12, 19-23): `load_definitions =
   /etc/rabbitmq/definitions.json` (línea 12) permanece exactamente igual que antes
   de esta feature (confirmado por `git diff`, ningún cambio en esa línea ni en su
   comentario explicativo de la línea 1-11). Añadido debajo, sin tocar lo anterior:
   `listeners.ssl.default = 5671` y `ssl_options.cacertfile/certfile/keyfile`
   apuntando a `/etc/rabbitmq/tls/{ca_certificate,server_certificate,server_key}.pem`
   — rutas correctas dentro del contenedor (coinciden con el mount de
   `docker-compose.yml` y con `tests/common/mod.rs`). Correcto.

2. **`rabbitmq/generate-lab-certs.sh`**: genera una CA de laboratorio propia
   (`openssl req -x509 ... -subj "/C=CL/O=security-app-lab/OU=broker/CN=security-app-lab-CA"`)
   y un certificado de servidor firmado por ella vía CSR
   (`openssl x509 -req ... -CA ca_certificate.pem -CAkey ca_key.pem`), no un
   autofirmado suelto — hay cadena real CA→servidor. SAN
   `DNS:rabbitmq,DNS:localhost,IP:127.0.0.1` cubre los tres casos necesarios
   (contenedor interno, host, loopback). El `CN`/`O` (`security-app-lab`,
   `security-app-lab-CA`, `CN=rabbitmq` genérico sin dominio real) no podría
   confundirse con un certificado real de producción. El script y `rabbitmq/README.md`
   documentan "NO usar en producción" de forma explícita y repetida.

3. **`docker-compose.yml`**: puerto `"5671:5671"` expuesto (línea 37), volumen
   `./rabbitmq/tls:/etc/rabbitmq/tls:ro` montado (línea 42). Puerto `5672` sigue
   presente (línea 36) con comentario inline actualizado
   (`# AMQP en claro — solo desarrollo/depuración local, ver nota arriba`) y el
   comentario de cabecera (líneas 8-17) deja explícito que desde esta feature AMQPS
   es la vía real y que "ningún servicio de producción debe apuntar a 5672" — no es
   solo el comentario viejo de `scaffolding`, fue reescrito para esta feature.

4. **Documentación de certs reales de producción**: `rabbitmq/README.md`
   §"Qué cambia para certificados reales de producción" (líneas 237-265) explica con
   precisión que las claves de `rabbitmq.conf` no cambian de nombre/forma, solo el
   origen de los archivos (CA real en vez de la de laboratorio, certificado/clave
   emitidos por esa CA), que la clave privada real nunca se commitea (vía secret
   management de la plataforma) y hasta anticipa mTLS como fuera de alcance. Cumple
   el criterio de aceptación explícito de `feature_list.json` para esta feature.

5. **Tests nuevos de TLS** (`tests/tls.rs`), ejecutados yo mismo vía `./init.sh`:
   - Test positivo `amqps_connection_with_lab_ca_operates_on_real_topology`: conecta
     por `amqps://` con la CA de laboratorio como usuario de servicio real
     (`lab-admin` para publicar, y **`ms-nmap`** — usuario de servicio, no admin —
     para leer su propia cola real `ms-nmap.scan-requests`), y valida el contenido
     exacto del mensaje recibido. Es más fuerte que una simple declaración pasiva:
     opera end-to-end sobre la topología real de la feature 2. `PASS` (confirmado en
     la corrida de `./init.sh`, ver log).
   - Test negativo `plaintext_connection_to_tls_port_is_rejected`: TCP crudo contra
     5671, protocol header AMQP en claro, verifica que el servidor nunca completa un
     handshake AMQP (distingue correctamente una respuesta con alerta TLS válida de
     un frame AMQP real, usando el primer octeto `ContentType` 20-23 vs el byte de
     método `0x01`). `PASS`.
   - Ambos están marcados `#[ignore = "requiere Docker"]`, convención correcta.

6. **Los 10 tests previos de la feature 2 siguen pasando sin cambios de
   comportamiento**: `git diff tests/common/mod.rs` confirma que `amqp_uri`/
   `connect_as` (usadas por `topology_exists.rs`, `permissions.rs`,
   `outcome_routing.rs`, `retry_delivery_limit.rs`) no cambiaron su lógica — el único
   cambio relacionado es que `start_broker()` ahora también monta los `.pem` de TLS
   (necesario porque `rabbitmq.conf` declara `ssl_options.*` incondicionalmente y el
   nodo no arranca sin ellos, ni siquiera el listener 5672 en claro; está bien
   documentado en el doc-comment de `start_broker`). `./init.sh` reporta **12/12**
   tests de integración en verde: los 10 anteriores + los 2 nuevos de `tls.rs`.
   `definitions.json` no tiene ningún diff (`git diff rabbitmq/definitions.json`
   vacío) — permisos/usuarios no se tocaron más allá de lo necesario para TLS.

7. **`./init.sh` en verde**: ejecutado yo mismo de punta a punta con Docker
   disponible. `fmt --check` OK, `clippy --all-targets -- -D warnings` OK, `cargo
   test` (0 ejecutados, ignorados correctamente marcados), `cargo test -- --ignored`
   → 12/12 verde (incluye el 1 de `topology_exists.rs`, 6 de `permissions.rs`, 2 de
   `outcome_routing.rs`, 1 de `retry_delivery_limit.rs`, 2 de `tls.rs`), `cargo doc
   --no-deps` OK. Resumen final: `[OK] Entorno listo`. Sin contenedores huérfanos al
   terminar (`docker ps -a --filter ancestor=rabbitmq:4.3.5-management` vacío).

8. **Ninguna credencial/clave real commiteada**: `git status --porcelain=v1 -uall`
   no muestra ningún `.pem`/`.key` como archivo nuevo — `rabbitmq/tls/*.pem`
   confirmados fuera de git vía `git check-ignore -v` (regla `*.pem` de
   `.gitignore`, ya existente desde `scaffolding`). Las contraseñas de laboratorio en
   `src/lib.rs`/`rabbitmq/README.md` (`lab-admin`, `lab-only-not-a-real-secret*`) son
   las mismas de la feature 2, sin cambios, y siguen claramente marcadas como
   ficticias.

9. **No se adelantó la feature 4**: `contracts/README.md` sigue siendo el mismo
   placeholder de `scaffolding` (sin diff, sin archivos nuevos en `contracts/`). No
   se tocaron permisos/usuarios de `definitions.json` (sin diff). `feature_list.json`
   solo cambió el `status` de la feature 3 a `"in_progress"` (no se marcó `done`,
   correcto según el protocolo del líder — queda para el reviewer/leader decidir el
   cierre).

## Observación menor (no bloqueante)

- `server_key.pem` se genera con `chmod 644` en vez del `640` propuesto originalmente
  por la investigación previa, documentado y justificado en el propio script y en
  `progress/impl_tls.md` (necesario para que el usuario `rabbitmq` del contenedor
  oficial, que no coincide con el UID del host, pueda leer la clave vía bind-mount).
  Aceptable **solo** porque es material de laboratorio desechable, y queda explícito
  como tal — no aplicaría a un despliegue real. No amerita rechazo.

## Checkpoints (recorrido de `CHECKPOINTS.md`)

- C1: [x] — arnés completo, `./init.sh` exit 0.
- C2: [x] — una sola feature `in_progress` (`tls`, id 3); `progress/current.md`
  describe la sesión activa sin basura vieja.
- C3: [x] — solo los directorios previstos; `definitions.json` sin credenciales
  reales ni tocado; permisos acotados sin cambios; `contracts/` sin adelantar;
  toda dependencia nueva de `Cargo.toml` (`rustls` en dev-dependencies, features
  `net`/`io-util` de `tokio`) justificada y documentada por esta feature; sin
  `println!`/`dbg!`/`unwrap()` fuera de tests.
- C4: [x] — tests de integración reales contra `rabbitmq:management` vía
  `testcontainers`; positivo+negativo de permisos ya cubiertos desde la feature 2 y
  siguen verdes; `cargo test`/`clippy` limpios; `docker compose config` válido.
- C5: [x] — sin archivos sospechosos sin trackear (solo los esperados de esta
  sesión: `progress/explore_*.md`, `progress/impl_tls.md`, `rabbitmq/generate-lab-certs.sh`,
  `tests/tls.rs`); sin contenedores huérfanos; `progress/current.md` refleja
  correctamente el estado de la última feature trabajada (nota: `progress/history.md`
  no fue evaluado como bloqueante aquí porque el cierre formal de sesión — mover
  `current.md` a `history.md` y marcar `done` — es responsabilidad del leader
  después de este review, no del implementer).

## Cambios requeridos

Ninguno.
