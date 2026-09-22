# Review — feature `topology_definition` (id 2)

**Veredicto:** APPROVED

## Verificación realizada (no solo lectura del informe)

- Leídos íntegros: `docs/architecture.md`, `docs/conventions.md`,
  `docs/security-scope.md`, `docs/verification.md`, `CHECKPOINTS.md`,
  criterios exactos de `feature_list.json` id 2, e
  `progress/impl_topology_definition.md`.
- Inspeccionado el contenido real de `rabbitmq/definitions.json`,
  `rabbitmq/rabbitmq.conf`, `docker-compose.yml`, `rabbitmq/README.md`,
  `Cargo.toml`, `src/lib.rs`, `tests/common/mod.rs`,
  `tests/topology_exists.rs`, `tests/permissions.rs`,
  `tests/outcome_routing.rs`, `tests/retry_delivery_limit.rs` — no se
  confió en el resumen del implementer.
- Ejecutado `./init.sh` completo (con Docker real disponible en el
  entorno): `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, `cargo test -- --ignored`, `cargo doc --no-deps`.
  Resultado: **verde de punta a punta**, `[OK] Entorno listo.` Los 10 tests
  de integración corrieron de verdad contra Docker (no se limitaron a
  compilar/omitirse): 2/2 `outcome_routing`, 6/6 `permissions`, 1/1
  `retry_delivery_limit`, 1/1 `topology_exists`.
- Levantado el stack real (`docker compose up -d`) y revisados los logs:
  confirmado que `load_definitions` (vía `rabbitmq.conf`, sin prefijo
  `management.`) carga correctamente los 4 usuarios, el vhost
  `security-app`, los 4 exchanges, las 6 colas (con Raft leader election de
  las 3 quorum) y los 7 bindings **en el primer arranque** — se bajó con
  `docker compose down -v` sin dejar contenedores/volúmenes huérfanos.
- `docker ps -a` tras la ejecución de tests: sin contenedores de
  `testcontainers` huérfanos (se limpian solos).
- `git status`/`git diff` revisados: `feature_list.json` solo cambia el
  `status` de la feature 2 a `in_progress` (correcto, no se marcó `done`);
  ningún archivo fuera del alcance de la feature fue tocado;
  `contracts/README.md` sigue intacto, sin adelantar la feature
  `message_contract` (id 4); no se tocó TLS/puerto 5671 (feature `tls`, id
  3, sigue `pending`), y el puerto 5672 en claro queda explícitamente
  documentado como solo-dev en el propio `docker-compose.yml`.
- `grep` de credenciales/secretos en `definitions.json`, `docker-compose.yml`
  y `tests/*.rs`/`src/lib.rs`: solo aparecen los valores de laboratorio
  claramente marcados (`lab-only-not-a-real-secret*`) y sus
  `password_hash` correspondientes — ningún secreto real, ninguna
  credencial SSH de ejemplo con apariencia real.

## Cobertura punto por punto de lo pedido en el encargo

1. **Topología exacta** — confirmada en `definitions.json` y verificada en
   vivo (`docker compose up`): vhost `security-app`; `scan.requests`
   (topic) + `scan.requests.dlx` (direct); `scan.outcomes` (topic) +
   `scan.outcomes.dlx` (direct). Coincide con `docs/conventions.md` y el
   criterio de aceptación.
2. Cola `ms-nmap.scan-requests` bindeada a `scan.requests` con routing key
   `scan.request`, con `ms-nmap.scan-requests.dlq` bindeada a
   `scan.requests.dlx` — confirmado en `definitions.json` líneas 74–85,
   170–186, y probado por `tests/topology_exists.rs`.
3. Cola `ms-analisis.scan-outcomes` bindeada **solo** con
   `scan.outcome.completed`/`scan.outcome.failed` (`definitions.json`
   líneas 187–202). `tests/topology_exists.rs` línea 207–215 verifica
   explícitamente la **ausencia** del binding con `scan.outcome.started`, y
   `tests/outcome_routing.rs::started_reaches_only_gateway_queue` lo
   confirma en runtime (un `scan.outcome.started` publicado nunca aparece
   en `ms-analisis.scan-outcomes`).
4. Cola `gateway.scan-outcomes` bindeada con `scan.outcome.#`
   (`definitions.json` línea 203–210), verificado en runtime por
   `tests/outcome_routing.rs::completed_reaches_both_queues` (llega a
   ambas colas) y por el propio `started_reaches_only_gateway_queue`.
5. **Reintentos acotados (RNF-06)**: `x-delivery-limit: 3` en las 3 colas
   principales, todas `x-queue-type: quorum`. La desviación de usar
   `basic.reject` en vez de `basic.nack` está bien fundamentada, no es una
   improvisación: `tests/retry_delivery_limit.rs` documenta la
   investigación empírica (contra RabbitMQ 4.3.5 real: `basic.nack`
   nunca incrementa `x-delivery-count`, solo `basic.reject` lo hace), y el
   test demuestra un **ciclo real de reintentos** —
   `redeliveries > 1` se afirma explícitamente antes de comprobar la
   `.dlq` (línea 148–152 del test), y se confirmó ejecutándolo: pasó en
   verde. El mensaje aparece en `ms-nmap.scan-requests.dlq` con
   `x-death[].reason == "delivery_limit"`, prueba concluyente de que pasó
   por el mecanismo de límite de entregas y no cayó directo a la `.dlq`. La
   elección de `x-delivery-limit` (mecanismo nativo de colas quorum) frente
   al patrón clásico de "cola de retry + TTL" que sugiere el criterio de
   aceptación como *ejemplo* (`p. ej.`) está justificada por escrito en
   `rabbitmq/README.md` §"Reintentos acotados" — cumple el requisito real
   (N acotado, documentado, no un rechazo único a la `.dlq`, contabilizado
   vía el mismo header `x-death` que el patrón clásico usa). N=3 tiene
   justificación explícita ("3 strikes", comparación con el default de
   RabbitMQ 4.0+ de 20), no es un valor arbitrario.
6. **Permisos de mínimo privilegio** — verificado línea a línea en
   `definitions.json` (bloque `permissions`) y confirmado en runtime por
   los logs de `docker compose up` (`Successfully set permissions for user
   'gateway' ... '^$', '^scan\.requests$', '^gateway\.scan-outcomes$'`,
   etc.):
   - `gateway`: `write: ^scan\.requests$`, `read:
     ^gateway\.scan-outcomes$`, `configure: ^$`. Casos negativos
     (`tests/permissions.rs::gateway_cannot_read_ms_nmap_or_ms_analisis_queues`)
     verifican `403 ACCESS_REFUSED` real leyendo `ms-nmap.scan-requests` y
     `ms-analisis.scan-outcomes` — ambos casos negativos cubiertos, no solo
     uno.
   - `ms-nmap`: `write: ^scan\.outcomes$`, `read:
     ^ms-nmap\.scan-requests$`, `configure: ^$`. Caso negativo
     (`ms_nmap_cannot_write_scan_requests_or_read_other_queues`) cubre los
     3 sub-casos: no puede escribir en `scan.requests` (403 confirmado con
     `assert_write_denied`, que fuerza una RPC síncrona posterior para
     detectar el cierre de canal), no puede leer `gateway.scan-outcomes` ni
     `ms-analisis.scan-outcomes`.
   - `ms-analisis`: `write: ^$`, `read: ^ms-analisis\.scan-outcomes$`,
     `configure: ^$`. Caso negativo
     (`ms_analisis_cannot_write_anything_or_read_other_queues`) cubre 4
     sub-casos: no escribe en `scan.outcomes` ni en `scan.requests`, no lee
     `gateway.scan-outcomes` ni `ms-nmap.scan-requests`.
   - Ningún usuario de servicio tiene acceso cruzado a la infraestructura
     de otro. No se detectó ninguna fuga de privilegio.
7. Ninguna credencial real ni parecida a una real — confirmado por
   inspección directa y `grep`; los 4 `password_hash` son valores
   calculados de contraseñas de laboratorio explícitamente marcadas
   (`lab-only-not-a-real-secret*`), documentadas también en `src/lib.rs`
   (única fuente en texto claro, coherente con el hecho de que
   `definitions.json` solo admite hashes).
8. `docker-compose.yml` monta `rabbitmq/definitions.json` y
   `rabbitmq/rabbitmq.conf`; se confirmó en vivo (no solo leyendo el
   YAML) que el mecanismo `load_definitions` realmente carga la topología
   completa en el primer arranque del contenedor real.
9. Los 10 tests de integración se ejecutaron aquí mismo contra Docker real
   (no se asumió por el informe) — 10/10 en verde.
10. `./init.sh` verde: `fmt`, `clippy -D warnings`, `test`, `test
    --ignored`, `doc` — confirmado con ejecución propia, no solo citando el
    informe.
11. No se adelantó TLS (puerto 5672 en claro, explícitamente documentado
    como solo-dev en `docker-compose.yml`, coherente con
    `docs/security-scope.md` y la feature `tls` aún `pending`) ni el
    contrato de mensajes (`contracts/` sigue con solo su `README.md`
    placeholder de la feature `scaffolding`, sin tocar).

## Observación menor (no bloqueante)

- `futures = "0.3.34"` en `Cargo.toml` no se usa en ningún archivo del
  crate (`grep -rn "futures" tests/ src/` solo encuentra la línea del
  propio `Cargo.toml`). No rompe `clippy`/`fmt`/tests, pero es una
  dependencia sin uso real todavía — CHECKPOINTS C3 pide que "toda
  dependencia... esté justificada por una feature". No amerita rechazar la
  feature (no es un problema de seguridad ni de arquitectura), pero el
  implementer debería, en la próxima sesión que toque `Cargo.toml`,
  eliminarla si sigue sin usarse o documentar para qué se reserva.

## Checkpoints (evaluados contra el estado actual del repo, no solo esta feature)

- C1: [x] — `AGENTS.md`, `init.sh`, `feature_list.json`,
  `progress/current.md`, los 4 docs y `./init.sh` terminan con exit 0
  (verificado con ejecución real).
- C2: [x] — 1 sola feature `in_progress` (`topology_definition`, id 2);
  toda feature `done` (`scaffolding`) sigue con su verificación previa
  intacta; `progress/current.md` refleja la sesión activa sin basura de
  sesiones previas.
- C3: [x] — solo existen los directorios previstos
  (`docker-compose.yml`, `rabbitmq/`, `contracts/`, `src/`, `tests/`);
  `definitions.json` sin credenciales reales; permisos de cada usuario
  acotados exactamente a lo que necesita, sin acceso cruzado (verificado
  con los 6 tests positivos/negativos y en runtime); `contracts/` sin
  tocar (feature 4 aún pendiente); dependencias de `Cargo.toml`
  justificadas salvo `futures` (ver observación menor arriba, no
  bloqueante); sin `println!`/`dbg!`/`unwrap()` fuera de tests (el crate
  usa `expect()`/`panic!()` solo dentro de `tests/`, permitido por
  `docs/conventions.md`).
- C4: [x] — tests de integración reales contra `rabbitmq:4.3.5-management`
  vía `testcontainers`, nunca mocks ni producción; permisos cubren
  positivo+negativo para los 3 usuarios; `cargo test`/`cargo test
  --ignored` muestran 10/10 tests verdes; `cargo clippy --all-targets -- -D
  warnings` sin advertencias; `docker compose config` valida sin error.
- C5: [x] — sin archivos sospechosos sin trackear (solo los artefactos
  esperados de esta sesión, listados en `progress/impl_topology_definition.md`
  y confirmados con `git status`); sin contenedores/volúmenes Docker
  huérfanos tras la verificación; `progress/current.md` documenta la
  sesión activa con su bitácora; la feature queda correctamente en
  `in_progress` (no se marcó `done` — corresponde al leader tras este
  veredicto).

## Conclusión

Implementación sólida, coherente con `docs/architecture.md`,
`docs/conventions.md` y `docs/security-scope.md`. Los usuarios de servicio
están correctamente aislados por mínimo privilegio (sin fugas de permisos
detectadas, con los 3 casos negativos probados de verdad contra el broker
real, no solo asumidos). El uso de `x-delivery-limit` en vez del patrón
"cola de retry + TTL" está bien justificado y verificado empíricamente
contra un broker real, con evidencia de que el mensaje atraviesa un ciclo
real de reintentos antes de caer en la `.dlq`. `./init.sh` termina en
verde con los 10 tests de integración corriendo contra Docker real. No se
adelantó ninguna feature fuera de alcance (TLS, contrato de mensajes).

**No hay cambios requeridos.** La única observación (dependencia `futures`
sin usar) es menor y no bloquea el cierre de esta feature.
