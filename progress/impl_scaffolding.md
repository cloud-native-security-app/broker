# Implementación — feature 1 `scaffolding`

## Archivos creados

- `docker-compose.yml` (raíz): servicio `rabbitmq` con imagen
  `rabbitmq:4.3.5-management` (tag concreto, verificado que existe en Docker
  Hub el 2026-09-14, no `latest`). Expone `5672:5672` (AMQP, comentado
  explícitamente como solo desarrollo/depuración local, TLS pendiente de la
  feature `tls`) y `15672:15672` (UI de management). Variables
  `RABBITMQ_DEFAULT_USER=lab-admin` / `RABBITMQ_DEFAULT_PASS=lab-only-not-a-real-secret`,
  con comentario explícito de que son credenciales de laboratorio, no
  válidas para producción. No carga `rabbitmq/definitions.json` todavía
  (eso es la feature `topology_definition`, aún no existe el archivo).
- `rabbitmq/README.md`: placeholder explicando que este directorio
  contendrá `definitions.json` (vhost, exchanges, colas, bindings, DLQ,
  usuarios) a partir de las features `topology_definition` (id 2) y
  `cancellation_contract` (id 5).
- `contracts/README.md`: placeholder explicando que este directorio
  contendrá los JSON Schema de `ScanRequest`/`ScanOutcome` (feature
  `message_contract`, id 4) y `ScanCancellation` (feature
  `cancellation_contract`, id 5).
- `Cargo.toml`: crate `broker-verification`, `edition = "2021"`,
  `publish = false`, sin sección `[[bin]]`, sin dependencias (`[dependencies]`
  vacío) — lo mínimo para que el crate compile.
- `src/lib.rs`: archivo vacío (solo un doc-comment de módulo explicando el
  propósito del crate y que las features siguientes añadirán helpers
  puros), necesario para que `Cargo.toml` tenga un target válido.
- `Cargo.lock`: generado automáticamente por `cargo` al compilar (no
  contiene dependencias externas, solo el propio paquete).

## Archivos NO modificados (ya cumplían el criterio)

- `.gitignore`: ya existía en el repo (commit previo) y ya cubre
  `/target`, `*.tmp`, `*.pem`, `*.key`, `.env`. No requirió cambios.

## Decisiones de diseño

- **Tag de imagen**: `rabbitmq:4.3.5-management` — se consultó el registro
  de Docker Hub (`registry.hub.docker.com/v2/repositories/library/rabbitmq/tags`)
  para confirmar que es un tag de versión de parche concreto y existente
  (no una rama móvil como `4-management` o `management`), en línea con
  "fijada por tag concreto (no `latest`)".
- **Nombre del crate**: `broker-verification`, para reflejar en el propio
  `Cargo.toml` que este crate es solo arnés de verificación (testcontainers
  + lapin en features siguientes), no una librería compartida ni un
  binario de producción — consistente con `docs/architecture.md` y
  `docs/conventions.md`.
- **Puerto AMQP en claro (5672) expuesto**: se documenta en un comentario
  del propio `docker-compose.yml` como solo desarrollo/depuración,
  anticipando la feature `tls` (id 3) que añadirá AMQPS (5671). No se dejó
  como implícito.
- **No se creó `tests/`**: la feature es puro andamiaje y la aceptación no
  lo exige (0 tests es válido); `docs/conventions.md` indica que los tests
  de integración con Docker empiezan en la feature `topology_definition`.
  Un directorio `tests/` vacío no lo trackea git de todos modos, así que se
  omitió para no dejar artefactos sin propósito inmediato.
- **No se tocó `rabbitmq/definitions.json`, usuarios de servicio ni
  `contracts/*.schema.json`**: fuera de alcance de esta feature (features 2
  y 4), tal como exige la descripción de la feature `scaffolding`.

## Verificación ejecutada

### `./init.sh`

Resultado: **verde de punta a punta** (exit code 0).

- Sección 1 (entorno): OK — cargo 1.98.0, rustc 1.98.0, Docker 29.7.2.
- Sección 2 (archivos base del arnés): OK.
- Sección 3 (`feature_list.json` válido): OK (7 features, 1 en
  `in_progress`).
- Sección 4 (`docker compose config`): OK.
- Sección 5 (crate Rust):
  - `cargo fmt --check`: sin diferencias.
  - `cargo clippy --all-targets -- -D warnings`: sin warnings.
  - `cargo test`: 0 tests, 0 failed (esperado, crate vacío).
  - `cargo test -- --ignored`: 0 tests, 0 failed (esperado).
  - `cargo doc --no-deps`: genera sin errores.
- Resumen final: `[OK] Entorno listo. Puedes empezar a trabajar.`

### `docker compose config`

Ejecutado explícitamente además de dentro de `init.sh`, sin errores.
Salida (resumen): un único servicio `rabbitmq` con la imagen
`rabbitmq:4.3.5-management`, puertos `5672` y `15672` publicados, y las dos
variables de entorno de laboratorio. YAML válido, sin advertencias.

## Estado de git al terminar

Untracked (pendientes de que el líder decida si/cuándo commitear):
`Cargo.lock`, `Cargo.toml`, `contracts/`, `docker-compose.yml`, `rabbitmq/`,
`src/`. No se hizo ningún `git add`/`git commit` — eso queda fuera del
alcance del implementer según el protocolo.

`feature_list.json` (feature id 1, `scaffolding`) y `progress/current.md`
ya estaban en estado `in_progress` desde antes de esta sesión (modificados
por el líder); no se cambió el `status` a `done` — corresponde al flujo de
revisión/cierre coordinado por el líder tras la aprobación del `reviewer`.

## Pendiente / bloqueos

Ninguno. Los 7 criterios de aceptación de la feature `scaffolding` están
cubiertos y verificados. Queda pendiente la revisión de un subagente
`reviewer` y, si aprueba, el cambio de `status` a `done` (fuera del alcance
de esta sesión de implementación).

## Cierre de sesión (2026-09-14)

- **Reviewer:** APPROVED, sin cambios requeridos (veredicto completo en
  `progress/review_scaffolding.md`).
- `./init.sh` re-ejecutado una vez más antes de cerrar: verde de punta a
  punta (secciones 1-6 OK, 0 tests válido).
- `feature_list.json`: feature id 1 (`scaffolding`) → `status: "done"`.
- `progress/current.md`: resumen movido a `progress/history.md` (entrada
  "2026-09-14 — Feature 1 `scaffolding` — DONE"); `progress/current.md`
  vaciado a la plantilla original (sin feature en curso).
- Docker: `docker ps -a` verificado — no hay ningún contenedor
  RabbitMQ/broker de esta sesión corriendo ni huérfano. Los contenedores
  presentes en la máquina pertenecen a otros proyectos y ya estaban
  detenidos desde antes.
- Sin archivos temporales sueltos ni artefactos sin propósito.
- Sesión cerrada según `AGENTS.md` §5.
