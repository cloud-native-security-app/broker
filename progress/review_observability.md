# Review — feature `observability` (id 6)

**Veredicto:** APPROVED

## Verificación realizada

- Leí `rabbitmq/definitions.json`, `rabbitmq/README.md`, `docker-compose.yml`,
  `docs/architecture.md`, `docs/conventions.md`, `docs/security-scope.md`,
  `docs/verification.md`, `CHECKPOINTS.md`, `feature_list.json`,
  `tests/observability.rs`, `tests/common/mod.rs`, `tests/retry_delivery_limit.rs`,
  `progress/current.md`, `progress/impl_observability.md`, `src/lib.rs`.
- Ejecuté `git diff --stat` y `git diff` sobre `feature_list.json` y
  `docs/architecture.md` para confirmar el alcance real de los cambios
  (no confié solo en el informe del implementer).
- Ejecuté `docker compose config` (exit 0, sin `15692` en los puertos
  publicados: solo `5672`, `5671`, `15672`).
- Ejecuté `./init.sh` completo (con Docker real disponible): **verde de
  punta a punta**, `[OK] Entorno listo`. Incluye `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (15 unitarios),
  `cargo test -- --ignored` (20/20 tests de integración, incluidos los 3
  nuevos de `tests/observability.rs`: `node_healthcheck_reports_ok`,
  `publishing_n_messages_reports_matching_queue_depth`,
  `exhausted_message_visible_in_dlq_via_management_api` — todos `ok`),
  `cargo doc --no-deps`.

## Hallazgos por punto de la tarea

1. **Documentación de endpoints** — `rabbitmq/README.md` §"Observabilidad"
   lista `GET /api/healthchecks/node` y `GET /api/queues/<vhost>/<queue>`, y
   enumera explícitamente las 8 colas reales (líneas 301-305): 4 principales
   (`ms-nmap.scan-requests`, `ms-analisis.scan-outcomes`,
   `gateway.scan-outcomes`, `ms-nmap.scan-cancellations`) + sus 4 `.dlq`.
   Confirmé leyendo `rabbitmq/definitions.json` directamente (bloque
   `"queues"`, líneas 73-149) que son exactamente esas 8 colas, sin
   inventos ni omisiones. `docs/architecture.md` (líneas 98-104) referencia
   correctamente esta sección.
2. **Decisión sobre `rabbitmq_prometheus`** — justificada por escrito y no
   implícita, en `rabbitmq/README.md` §"Decisión sobre el plugin
   `rabbitmq_prometheus`" (líneas 332-354): documenta el hallazgo empírico
   (el plugin viene habilitado por defecto en la imagen oficial desde
   RabbitMQ 3.8+, sirviendo en el puerto interno 15692) y la decisión real
   (no publicar `15692:15692`), con motivo razonable (no hay
   Prometheus/Grafana en este laboratorio; la API de management ya cubre
   RNF-09/RNF-10) y sin cerrar la puerta a una feature futura si se decide
   desplegar observabilidad basada en Prometheus. Verifiqué con
   `docker compose config` que el `docker-compose.yml` real solo publica
   `5672`, `5671` y `15672` — el puerto 15692 no aparece. `git diff --stat`
   confirma que `docker-compose.yml` no fue tocado en esta sesión.
3. **Test de healthcheck** — `node_healthcheck_reports_ok` ejecutado
   contra Docker real: `ok`. Verifica `200 OK` y `{"status":"ok"}` contra
   el contenedor con la topología cargada.
4. **Test de profundidad de cola** — `publishing_n_messages_reports_matching_queue_depth`
   ejecutado: `ok`. Publica N=3 mensajes en `scan.requests`/`scan.request`
   sin consumirlos y sondea `/api/queues/security-app/ms-nmap.scan-requests`
   hasta que `messages_ready == 3` (con `wait_for_messages_ready`,
   justificado empíricamente: las estadísticas de management no son
   instantáneas), y comprueba `consumers == 0`. Cada test de
   `tests/observability.rs` levanta su **propio contenedor** vía
   `common::start_broker()` (un `testcontainers::ContainerAsync` por test),
   así que no hay interferencia entre este test y
   `exhausted_message_visible_in_dlq_via_management_api` aunque ambos usen
   el nombre de cola `ms-nmap.scan-requests` — están aislados en
   contenedores efímeros distintos. Correcto.
5. **Test de mensaje en `.dlq`** — `exhausted_message_visible_in_dlq_via_management_api`
   ejecutado: `ok`. Reutiliza el mismo mecanismo ya probado en
   `tests/retry_delivery_limit.rs` (`basic.reject(requeue=true)` repetido
   hasta agotar `x-delivery-limit=3`, con el mismo helper `poll_get` y la
   misma justificación empírica de por qué `basic.reject` y no
   `basic.nack`), y verifica vía la API de management que
   `ms-nmap.scan-requests.dlq` reporta `messages_ready == 1` y que
   `ms-nmap.scan-requests` vuelve a `0`.
6. **Autenticación** — todas las consultas HTTP en `tests/observability.rs`
   usan `creds::ADMIN_USER`/`creds::ADMIN_PASSWORD` (`lab-admin` /
   `lab-only-not-a-real-secret`, confirmados en `src/lib.rs` líneas 37-38),
   nunca un usuario de servicio. `rabbitmq/README.md` documenta
   explícitamente que ningún usuario de servicio tiene el tag
   `administrator`, así que ni podría autenticarse contra la API.
7. **Fuera de alcance** — `git diff --stat` confirma que solo se modificaron
   `docs/architecture.md` (+3/-1 líneas, solo la referencia cruzada),
   `feature_list.json` (solo el `status` de la feature 6, ya puesto por el
   leader antes del dispatch, confirmado también por `git diff
   feature_list.json`), `progress/current.md` y `rabbitmq/README.md`; y se
   crearon `tests/observability.rs` y `progress/impl_observability.md`.
   `rabbitmq/definitions.json`, los permisos de usuarios, `rabbitmq.conf`
   (TLS) y `docker-compose.yml` no aparecen en el diff — no se tocó nada
   fuera del alcance de esta feature.
8. **`./init.sh` en verde** — confirmado por mí mismo, ver arriba. Los ~17
   tests de integración preexistentes (features 2-5: `cancellation.rs` ×3,
   `message_contract.rs` ×2, `outcome_routing.rs` ×2, `permissions.rs` ×6,
   `retry_delivery_limit.rs` ×1, `tls.rs` ×2, `topology_exists.rs` ×1) más
   los 3 nuevos de `observability.rs` = 20/20 verdes.
9. **Sin secretos en logs** — los payloads de `tests/observability.rs`
   son `{"correlation_id":"lab-only-observability-test"}` (y el mismo
   patrón `lab-only-...` de `retry_delivery_limit.rs`); no hay
   `ssh_credentials_ref` ni ningún cuerpo de mensaje real. No hay
   `println!`/`dbg!` en el archivo.

## Checkpoints (relevantes a esta sesión)

- C1: [x] — `./init.sh` exit code 0; existen los 4 archivos base y los 4 docs.
- C2: [x] — Solo la feature 6 está `in_progress`; `progress/current.md`
  describe la sesión activa sin basura de sesiones anteriores.
- C3: [x] — No se crearon directorios nuevos; `rabbitmq/definitions.json`
  no fue tocado (sigue sin credenciales reales); los permisos de usuarios
  no cambiaron; no hay `println!`/`dbg!`/`unwrap()` fuera de tests nuevos.
- C4: [x] — Los 3 tests nuevos corren contra `rabbitmq:4.3.5-management`
  real vía `testcontainers`, nunca mocks; `cargo test`/`clippy` limpios;
  `docker compose config` valida.
- C5: [ ] — No aplica a esta revisión (el cierre de sesión, con
  `progress/history.md` y el `status` final `done`, corresponde al leader
  tras este veredicto — la feature sigue en `in_progress` como corresponde
  hasta que se cierre formalmente).

## Cambios requeridos

Ninguno.
