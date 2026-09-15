# Review — feature 1 `scaffolding`

**Veredicto:** APPROVED

## Verificación realizada (no solo lectura de la bitácora)

- Leí `progress/impl_scaffolding.md`, `docs/architecture.md`,
  `docs/conventions.md`, `docs/security-scope.md`, `CHECKPOINTS.md` y
  `feature_list.json` (criterios de aceptación exactos de la feature id 1).
- Inspeccioné directamente el contenido real de `docker-compose.yml`,
  `Cargo.toml`, `src/lib.rs`, `rabbitmq/README.md`, `contracts/README.md`,
  `.gitignore` y la estructura completa del repo (`find . -maxdepth 2`).
- Ejecuté `./init.sh` yo mismo: exit code 0, secciones 1-5 en verde
  (`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` con 0 tests, `cargo test -- --ignored` con 0 tests,
  `cargo doc --no-deps`).
- Ejecuté `docker compose config` yo mismo: valida sin error, un único
  servicio `rabbitmq`, imagen `rabbitmq:4.3.5-management`, puertos
  `5672`/`15672`, variables `RABBITMQ_DEFAULT_USER`/`RABBITMQ_DEFAULT_PASS`
  de laboratorio.
- Verifiqué con `docker manifest inspect rabbitmq:4.3.5-management` (exit 0,
  manifest list real multi-arquitectura) que el tag **existe de verdad** en
  el registro — no es un tag inventado ni `latest`.
- Revisé `git status --porcelain`: no hay artefactos sueltos sospechosos
  (`*.tmp`, `target/` sin trackear, certificados de prueba). El único
  untracked es exactamente lo que la bitácora dice que creó, más
  `progress/impl_scaffolding.md`.

## Contraste contra criterios de aceptación (feature 1, `feature_list.json`)

1. `docker-compose.yml` con RabbitMQ, imagen `rabbitmq:4.3.5-management`
   fijada por tag concreto (no `latest`) — **cumple**, confirmado con
   `docker manifest inspect`.
2. Expone `5672` y `15672`; usuario `lab-admin`/`lab-only-not-a-real-secret`
   documentado explícitamente como credencial de laboratorio en comentarios
   del propio archivo (líneas 20-23 de `docker-compose.yml`) — **cumple**.
   No hay credenciales reales en ningún archivo tocado.
3. `rabbitmq/` y `contracts/` existen con `README.md` placeholder que
   explica qué contendrán las features 2, 4 y 5, sin adelantar
   `definitions.json` ni JSON Schemas reales — **cumple**. Verificado que
   ambos directorios solo contienen el README (`ls -la`).
4. `Cargo.toml`: `edition = "2021"`, `publish = false`, sin sección
   `[[bin]]`, `[dependencies]` vacío, sin `src/main.rs` — **cumple**, y es
   consistente con `docs/conventions.md` ("este crate no tiene un binario
   de producción").
5. `.gitignore` ya cubría `/target`, `*.tmp`, `*.pem`, `*.key` (más `.env`)
   desde antes de esta sesión — **cumple**, el implementer correctamente no
   lo tocó porque ya satisfacía el criterio.
6. `./init.sh` en verde de punta a punta, 0 tests válido para un crate vacío
   — **cumple**, confirmado por ejecución propia.
7. `docker compose config` valida sin error — **cumple**, confirmado por
   ejecución propia.

## Contraste contra `docs/architecture.md` / `docs/conventions.md`

- Respeta la topología de capas de la sección "Capas / directorios": solo
  existen `docker-compose.yml`, `rabbitmq/`, `contracts/`, `src/`
  (`Cargo.toml`, `Cargo.lock`) — ningún directorio adicional no previsto.
- No se adelantó topología real (`rabbitmq/definitions.json` no existe
  todavía), ni usuarios de servicio (`gateway`/`ms-nmap`/`ms-analisis`), ni
  contrato de mensajes (`contracts/*.schema.json` no existe) — correcto,
  eso es explícitamente fuera de alcance de `scaffolding` según su propia
  `description` en `feature_list.json` y según "Qué NO hacer" de
  `docs/architecture.md`.
- El puerto AMQP en claro (5672) queda marcado explícitamente como
  solo-desarrollo/depuración en comentarios del propio
  `docker-compose.yml`, tal como exige `docs/security-scope.md` §"TLS
  obligatorio" para el caso en que se deje expuesto en local.
- Nombre del crate (`broker-verification`) y ausencia de dependencias
  compartidas es coherente con la decisión de diseño "sin librería
  compartida" de `docs/architecture.md`.
- Estilo Rust: edition 2021 (cumple "2021 o superior"), sin `unwrap`/
  `expect`/`panic!` (no hay lógica todavía), `cargo fmt`/`clippy` limpios.

## Checkpoints (`CHECKPOINTS.md`)

Evaluados en el estado en que quedó el repo tras esta feature (algunos
checkpoints de C3/C4 dependen de topología/contrato/tests que son features
2, 4 y 5 — se marcan `[ ]` porque objetivamente no existen todavía, no
porque el scaffolding esté mal; es el estado esperado en este punto del
roadmap).

- C1: [x] Existen los 4 archivos base y los 4 docs; `./init.sh` exit 0.
- C2: [x] Solo la feature `scaffolding` está en `in_progress` en
  `feature_list.json`; `progress/current.md` describe la sesión activa sin
  basura de sesiones anteriores.
- C3: [ ] ← Razón: aún no aplica en su totalidad. Los directorios previstos
  existen y son correctos (`docker-compose.yml`, `rabbitmq/`, `contracts/`,
  `src/`); pero `rabbitmq/definitions.json` (permisos de mínimo privilegio,
  usuarios) y `contracts/` con el schema real todavía no existen — eso es
  trabajo de las features `topology_definition` (id 2) y `message_contract`
  (id 4), correctamente fuera de alcance aquí. No hay ninguna violación,
  solo checkboxes que no pueden cumplirse hasta esas features.
- C4: [ ] ← Razón: no hay `tests/` todavía (la feature `scaffolding` no lo
  exige; `docs/conventions.md` sitúa los tests de integración contra
  `rabbitmq:management` real a partir de `topology_definition`). No es un
  defecto de esta feature.
- C5: [x] No hay archivos sin trackear sospechosos; `progress/history.md`
  existe (vacío de entradas de sesión, consistente con que esta sesión aún
  no se cerró — se cerrará cuando el líder marque la feature `done`); el
  estado de la feature (`in_progress`, pendiente de aprobación del
  reviewer) está correctamente reflejado.

## Cambios requeridos

Ninguno. El trabajo del implementer para la feature `scaffolding` cumple
los 7 criterios de aceptación exactos de `feature_list.json`, respeta
`docs/architecture.md` y `docs/conventions.md`, no contiene credenciales
reales, no se adelanta a features futuras, y `./init.sh` +
`docker compose config` terminan en verde bajo ejecución propia del
reviewer (no solo según la bitácora del implementer).
