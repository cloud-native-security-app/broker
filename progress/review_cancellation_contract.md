# Review — feature 5 (cancellation_contract)

**Veredicto:** APPROVED

## Verificación realizada (no solo leí el informe del implementer)

- Leí `docs/architecture.md`, `docs/conventions.md`, `docs/security-scope.md`
  (incl. §"Cobertura de las features añadidas en la ronda 2"),
  `docs/verification.md`, `CHECKPOINTS.md` y los criterios de aceptación
  exactos de la feature id 5 en `feature_list.json`.
- Inspeccioné el contenido real (no el resumen) de: `rabbitmq/definitions.json`,
  `contracts/scan-cancellation.schema.json`, `contracts/README.md`,
  `rabbitmq/README.md`, `src/contracts.rs`, `tests/topology_exists.rs`,
  `tests/cancellation.rs`, `progress/current.md`.
- Ejecuté `git diff` archivo por archivo para confirmar que el diff real
  coincide con lo que el informe del implementer afirma (no hay
  discrepancias).
- Leí `nmap-service/src/messaging/consumer.rs` en el repo hermano
  (`/home/o-aguirre/Documents/duoc/cloud-native/security-app/nmap-service`)
  para confirmar independientemente la nota de "trabajo pendiente": el único
  uso de la palabra "cancel" en ese archivo es en un doc-comment sobre
  "suscripción cancelada" (línea 56, un caso genérico de fallo de
  transporte), no hay ningún tipo/trait/lógica para leer
  `ms-nmap.scan-cancellations` ni para abortar un escaneo. La nota del
  implementer en `contracts/README.md`/`rabbitmq/README.md` es honesta.
- Ejecuté `./init.sh` completo (con Docker real disponible) dos veces:
  termina con `[OK] Entorno listo. Puedes empezar a trabajar.` y exit code 0.
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` (15/15 unitarios, incluidos los 4 nuevos de
  `validate_scan_cancellation`), `cargo test -- --ignored` (17/17 de
  integración contra `rabbitmq:management` real vía `testcontainers`,
  incluidos los 3 nuevos de `tests/cancellation.rs` y los 14 preexistentes
  de las features 2-4, todos en verde), `cargo doc` sin errores,
  `docker compose config` válido.

## Hallazgos por punto de la tarea

1. **Topología nueva** — Confirmada en `rabbitmq/definitions.json`: exchange
   `scan.cancellations` (`topic`) con DLX `scan.cancellations.dlx`
   (`direct`); cola `ms-nmap.scan-cancellations` (`x-queue-type: quorum`,
   `x-delivery-limit: 3`, `x-dead-letter-exchange:
   scan.cancellations.dlx`, `x-dead-letter-routing-key:
   ms-nmap.scan-cancellations.dead`) — mismo patrón exacto que
   `ms-nmap.scan-requests`/`ms-analisis.scan-outcomes`/`gateway.scan-outcomes`
   de la feature 2, sin valores distintos sin justificar. Binding
   `scan.cancellations` → `ms-nmap.scan-cancellations` con routing key
   `scan.cancellation`, y `scan.cancellations.dlx` →
   `ms-nmap.scan-cancellations.dlq` con `ms-nmap.scan-cancellations.dead`.
   Cumple el criterio de aceptación de la feature 5 literalmente.

2. **Permisos (punto crítico)** — Verificado con `git diff
   rabbitmq/definitions.json`:
   - `gateway.write` pasó de `^scan\.requests$` a
     `^scan\.(requests|cancellations)$` — regex exacta, ancla `^...$`
     completa, solo dos alternativas literales, sin `.*` ni comodín
     accidental.
   - `ms-nmap.read` pasó de `^ms-nmap\.scan-requests$` a
     `^ms-nmap\.(scan-requests|scan-cancellations)$` — mismo patrón, sin
     ampliación de más.
   - El bloque de permisos de `ms-analisis` (líneas 61-67 de
     `rabbitmq/definitions.json`) está **ausente del diff**: no cambió una
     sola línea. Confirmado también en `tests/cancellation.rs::
     ms_analisis_cannot_write_or_read_cancellation_channel`, que corrió en
     verde contra Docker real (403 ACCESS_REFUSED tanto al publicar en
     `scan.cancellations` como al leer `ms-nmap.scan-cancellations`).
   - Ningún otro usuario (`lab-admin` es el admin de laboratorio, fuera de
     alcance de mínimo privilegio de servicio) tiene acceso a este
     exchange/cola. `ms_nmap_cannot_write_to_cancellation_exchange` confirma
     además que `ms-nmap` no ganó permiso de escritura sobre el canal que
     solo debe consumir.

3. **`contracts/scan-cancellation.schema.json`** — `correlation_id` y
   `requested_by`, ambos en `required`, `additionalProperties: false`. No
   hay ningún campo adicional (ni `schema_version` ni nada relacionado con
   la credencial SSH). Test unitario
   `scan_cancellation_with_unknown_extra_field_is_rejected` intenta colar
   explícitamente `ssh_credentials_ref` y confirma que el schema lo
   rechaza — verificación directa de la regla de seguridad central del
   repo.

4. **`contracts/README.md`** — Nueva sección `## ScanCancellation`
   documenta exchange/routing-key/publicador(Gateway)/consumidor(ms-nmap)
   con el mismo formato que `ScanRequest`/`ScanOutcome`. Subsección
   "Consumo y cancelación real — trabajo PENDIENTE en `nmap-service`"
   presente y, como describí arriba, confirmada independientemente contra
   el código real de `nmap-service/src/messaging/consumer.rs` — la nota no
   es una suposición, es correcta.

5. **Tests unitarios** — Ejecuté `cargo test` yo mismo: 15/15 pasan,
   incluidos `valid_scan_cancellation_passes`,
   `scan_cancellation_missing_correlation_id_is_rejected` (mensaje de error
   incluye el nombre del schema violado),
   `scan_cancellation_missing_requested_by_is_rejected`, y el test de
   campo extra ya mencionado en el punto 3.

6. **Test de integración** — Ejecuté `cargo test -- --ignored` yo mismo con
   Docker real: `gateway_can_publish_cancellation_and_it_arrives_at_ms_nmap_queue`
   publica como `gateway`, valida el payload contra el schema antes de
   publicar, y confirma la entrega íntegra leyendo como `lab-admin` (nunca
   como `gateway`, que correctamente no tiene permiso de lectura sobre esa
   cola — el informe documenta este detalle de diseño correctamente).
   `ms_analisis_cannot_write_or_read_cancellation_channel` cubre el caso
   negativo exigido por el acceptance list con `is_amqp_soft_error()`
   (403). Los 3 tests de `tests/cancellation.rs` pasaron en verde.

7. **`tests/topology_exists.rs` actualizado** — Comparé el conteo real
   (6 exchanges, 8 colas = 4 principales + 4 `.dlq`, 9 bindings) contra
   `rabbitmq/definitions.json`: coincide exactamente, sin huecos ni
   duplicados. El test hace comparación exacta de conjuntos
   (`assert_eq!`), por lo que cualquier discrepancia habría fallado en
   rojo — y no falló.

8. **`./init.sh` en verde** — Confirmado dos veces, exit code 0 en ambas
   corridas, con los ~25 tests previos de las features 2-4 más los nuevos
   de la feature 5, todos pasando contra Docker real (testcontainers).

9. **Fuera de alcance no tocado** — `git diff --stat` muestra exactamente
   7 archivos modificados (`contracts/README.md`, `feature_list.json`,
   `progress/current.md`, `rabbitmq/README.md`, `rabbitmq/definitions.json`,
   `src/contracts.rs`, `tests/topology_exists.rs`) + 3 nuevos
   (`contracts/scan-cancellation.schema.json`,
   `progress/impl_cancellation_contract.md`, `tests/cancellation.rs`). No
   se tocó `docker-compose.yml`, `rabbitmq/rabbitmq.conf`, `rabbitmq/tls/`,
   ni ningún archivo relacionado con TLS. El único cambio en
   `feature_list.json` es `status: "pending"` → `"in_progress"` de la
   propia feature 5 (correcto, no se marcó `done` — eso le corresponde al
   `leader`/`implementer` en un dispatch posterior tras este veredicto).
   No hay credenciales reales ni en `definitions.json` (solo hashes de
   laboratorio, sin cambios) ni en los payloads de ejemplo de los tests
   nuevos (`req-2026-0042`, `analyst@example.test`, consistentes con los ya
   usados en el repo).

## Checkpoints (CHECKPOINTS.md)

- C1: [x] — Existen los 4 archivos base y los 4 docs; `./init.sh` termina
  con exit code 0.
- C2: [x] — Solo la feature 5 está en `in_progress`
  (`grep '"status"' feature_list.json`: 4×`done`, 1×`in_progress`,
  2×`pending`); toda feature `done` mantiene sus tests pasando;
  `progress/current.md` describe la sesión activa sin basura de sesiones
  previas.
- C3: [x] — Solo existen los directorios previstos; `definitions.json` sin
  credenciales reales; cada usuario acotado a lo suyo (verificado punto 2);
  `contracts/` coincide con lo acordado (sin campos inventados); no hay
  `println!`/`dbg!`/`unwrap()` fuera de tests en el código nuevo (revisado
  `src/contracts.rs`).
- C4: [x] — Tests de integración reales contra `rabbitmq:management`
  (`testcontainers`); casos positivo y negativo de permisos presentes;
  `cargo test`/`clippy` en verde; `docker compose config` válido.
- C5: [x] — Sin archivos sueltos sospechosos (`git status` limpio salvo el
  entregable de la feature); `progress/current.md` refleja la sesión
  activa correctamente (la entrada a `history.md` y el cierre de sesión son
  responsabilidad del `leader` tras este veredicto, no de esta revisión).

## Cambios requeridos

Ninguno. El trabajo cumple literalmente los 6 criterios de aceptación de la
feature 5 en `feature_list.json`, respeta `docs/architecture.md`
(capas/alcance, nota de "canal y contrato, no consumo real"),
`docs/conventions.md` (nombres, regex de permisos, estilo Rust/tests) y
`docs/security-scope.md` (mínimo privilegio verificado, sin credenciales,
sin ampliación de `ms-analisis`). `./init.sh` termina en verde con toda la
suite (unitaria + integración con Docker real) en verde.
