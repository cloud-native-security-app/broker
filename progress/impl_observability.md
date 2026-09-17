# Implementación — feature `observability` (id 6)

## Resumen

Health checks del nodo y métricas de cola (RNF-09/RNF-10) vía la API HTTP
de management de RabbitMQ, sin tocar topología, permisos ni
`docker-compose.yml`. Feature de solo lectura/observabilidad sobre lo ya
existente (features 2 y 5).

## Archivos creados

- `tests/observability.rs` — 3 tests de integración
  `#[ignore = "requiere Docker"]`, contra `testcontainers` +
  `tests/common/mod.rs` (`start_broker`, `connect_as`,
  `management_base_url`), reutilizando `broker_verification::lab_credentials`
  y `broker_verification::VHOST`:
  - `node_healthcheck_reports_ok`: `GET /api/healthchecks/node` → `200 OK`,
    body `{"status":"ok"}`.
  - `publishing_n_messages_reports_matching_queue_depth`: publica 3
    mensajes en `scan.requests`/`scan.request` sin consumirlos y verifica
    que `/api/queues/security-app/ms-nmap.scan-requests` reporta
    `messages_ready: 3` y `consumers: 0`.
  - `exhausted_message_visible_in_dlq_via_management_api`: reutiliza el
    mecanismo de `tests/retry_delivery_limit.rs` (publicar +
    `basic.reject` ×3 hasta agotar `x-delivery-limit=3`) y verifica, vía
    `/api/queues/security-app/ms-nmap.scan-requests.dlq`, que
    `messages_ready` refleja el mensaje muerto (y que la cola de trabajo
    vuelve a 0).
  - Se añadió un helper de sondeo HTTP (`wait_for_messages_ready`,
    timeout 15s / intervalo 500ms) porque, verificado empíricamente contra
    un contenedor `rabbitmq:4.3.5-management` real (fuera del arnés, con
    `docker run` + `curl`/`python3` manuales antes de escribir el test),
    las estadísticas de la API de management **no son instantáneas**:
    tardaron hasta ~2s en reflejar publicaciones recientes en
    `messages_ready`, incluso ya pasado 1s. Sin este sondeo el test 2 y el
    test 3 habrían sido intermitentes (flaky).

## Archivos modificados

- `rabbitmq/README.md` — nueva sección "Observabilidad (health checks y
  métricas de colas, feature `observability`)": tabla de los 2 endpoints
  usados con ejemplo de respuesta real, lista explícita de las 8 colas a
  consultar (4 principales + 4 `.dlq`, confirmadas leyendo
  `rabbitmq/definitions.json` directamente, no asumidas), nota de
  autenticación (siempre `lab-admin`, nunca un usuario de servicio — que
  ni siquiera tiene el tag `administrator` para autenticarse contra la
  API), nota de "qué NO expone esta feature" (conteos/metadatos, nunca
  cuerpo de mensaje), la decisión sobre `rabbitmq_prometheus` (ver abajo)
  y el resumen de los 3 tests.
- `docs/architecture.md` — una línea añadida al bullet existente de
  observabilidad (RNF-09/RNF-10) apuntando a la documentación detallada en
  `rabbitmq/README.md` §"Observabilidad", para que quien lea
  `docs/architecture.md` primero encuentre el detalle sin tener que
  adivinar dónde vive.
- `progress/current.md` — bitácora de la sesión (decisiones, verificación
  empírica previa, resultado de `./init.sh`).
- `feature_list.json` — sin cambios de mi parte más allá del
  `status: "in_progress"` que ya había puesto el leader antes de
  despacharme (confirmado con `git diff`, no lo toqué).

## Decisión sobre el plugin `rabbitmq_prometheus` (criterio de aceptación explícito)

**No se publica ningún puerto adicional para él en `docker-compose.yml`
(sin cambios en ese archivo).**

Antes de decidir, levanté un contenedor real
(`docker run rabbitmq:4.3.5-management` con `rabbitmq/definitions.json` +
`rabbitmq/rabbitmq.conf` + `rabbitmq/tls/` montados, igual que
`docker-compose.yml`/`tests/common/mod.rs`, pero fuera del arnés de test,
solo para inspeccionar) y corrí `rabbitmq-plugins list -e` dentro:

```
[E*] rabbitmq_management       4.3.5
[e*] rabbitmq_management_agent 4.3.5
[E*] rabbitmq_prometheus       4.3.5
[e*] rabbitmq_web_dispatch     4.3.5
```

Hallazgo relevante (no asumido, verificado): **`rabbitmq_prometheus` ya
viene habilitado por defecto** en la imagen oficial `-management` desde
RabbitMQ 3.8+, junto con `rabbitmq_management` — no es algo que este repo
tenga que "habilitar" en `rabbitmq.conf`/`definitions.json`. Sirve métricas
en el puerto interno 15692 del contenedor, que **no está publicado** en
`docker-compose.yml` (a diferencia de 5672/5671/15672).

Justificación de no publicar `15692:15692`: no hay ningún
Prometheus/Grafana desplegado en este repo de laboratorio que vaya a
scrapear ese puerto, y esta feature ya cubre RNF-09/RNF-10 por completo
con la API HTTP de management (`/api/healthchecks/node`, `/api/
queues/...`), la misma vía que ya usan `tests/topology_exists.rs` y ahora
`tests/observability.rs`. Publicar un puerto adicional sin ningún
consumidor real solo ampliaría la superficie expuesta del contenedor de
desarrollo sin beneficio de observabilidad adicional ahora mismo. Si en el
futuro se decide desplegar Prometheus/Grafana, es una feature nueva a
discutir explícitamente (qué se publica, con qué autenticación/red, y su
propio análisis de `docs/security-scope.md`) — no se asume aquí. Detalle
completo en `rabbitmq/README.md` §"Decisión sobre el plugin
`rabbitmq_prometheus`".

## Verificación (`./init.sh`)

Verde de punta a punta, incluida la sección 5 completa:

- `cargo fmt --check` — sin diferencias.
- `cargo clippy --all-targets -- -D warnings` — sin warnings.
- `cargo test` (sin Docker) — 15 tests unitarios de `src/contracts.rs`
  (sin cambios, ya existentes de las features 4/5) + 20 tests de
  integración listados como `ignored, requiere Docker` (17 ya existentes
  de las features 2-5 + los 3 nuevos de esta feature).
- `cargo test -- --ignored` (con Docker real) — **todos verdes**, 20/20
  tests de integración pasan, incluidos:
  - los 17 preexistentes (`cancellation.rs` ×3, `message_contract.rs` ×2,
    `outcome_routing.rs` ×2, `permissions.rs` ×6, `retry_delivery_limit.rs`
    ×1, `tls.rs` ×2, `topology_exists.rs` ×1),
  - los 3 nuevos de `tests/observability.rs`.
- `cargo doc --no-deps` — genera sin errores.
- `docker compose config` — válido, sin cambios en `docker-compose.yml`.

Corrí `./init.sh` completo dos veces (una para ver el resumen final, otra
para confirmar la salida completa de la sección 1-5) — ambas en verde,
`[OK] Entorno listo`.

## Desviaciones del brief

Ninguna relevante. Una precisión frente a la sugerencia inicial del líder:
la sugerencia enmarcaba la decisión como "no habilitar el plugin porque no
hay Prometheus desplegado" — la investigación mostró que el plugin **ya
viene habilitado por la imagen base**, así que la decisión real y
documentada es específicamente sobre **no publicar el puerto 15692**, no
sobre "habilitar o no" el plugin en sí (eso ya lo decide la imagen
oficial). El resultado práctico (no hay superficie Prometheus expuesta
desde este repo) es el mismo que sugería el líder, pero la justificación
escrita en `rabbitmq/README.md` refleja el hallazgo real en vez de asumir
que el plugin estaba deshabilitado.

## Pendiente / bloqueos

Ninguno. Working tree limpio salvo los archivos listados arriba. Docker
estuvo disponible durante toda la sesión (contenedores de prueba
manuales limpiados con `docker rm -f` tras cada inspección).

No marqué `feature_list.json` como `done` — queda en `in_progress`,
pendiente del veredicto del `reviewer` y de que el líder cierre la
sesión según `AGENTS.md` §5.

## Cierre de sesión (2026-09-17)

Veredicto del reviewer: **APPROVED, sin cambios requeridos**
(`progress/review_observability.md`). Verificó por su cuenta la decisión
sobre `rabbitmq_prometheus`, corrió `./init.sh` completo contra Docker real
(20/20 tests de integración, incluidos los 3 nuevos de
`tests/observability.rs`) y confirmó vía `git diff --stat` que no se tocó
nada fuera del alcance de la feature.

Pasos de cierre ejecutados siguiendo `AGENTS.md` §5:

1. `./init.sh` re-ejecutado una vez más de punta a punta (con Docker
   disponible): verde, `[OK] Entorno listo`. `cargo test` → 15 unitarios
   ok; `cargo test -- --ignored` → 20/20 tests de integración ok (17
   preexistentes de las features 2-5 + los 3 nuevos de `observability.rs`:
   `node_healthcheck_reports_ok`,
   `publishing_n_messages_reports_matching_queue_depth`,
   `exhausted_message_visible_in_dlq_via_management_api`). `fmt`/`clippy -D
   warnings`/`doc` limpios.
2. `feature_list.json` id 6 (`observability`) → `status: "done"`.
3. Resumen movido al final de `progress/history.md` (sección
   "2026-09-17 — Feature 6 `observability` — DONE"), documentando: qué se
   documentó (endpoints de management para las 8 colas reales), la
   decisión sobre `rabbitmq_prometheus` (habilitado por defecto en la
   imagen, puerto 15692 no publicado, justificado), los 3 tests nuevos, y
   el veredicto del reviewer.
4. `progress/current.md` vaciado, dejando solo la plantilla original (sin
   cambios respecto al commit previo, confirmado por `git status`).
5. `docker ps -a` no muestra ningún contenedor de este repo (los
   contenedores listados pertenecen a otros proyectos ajenos, ya
   detenidos desde antes de esta sesión); `docker compose ps -a` está
   vacío — no había ningún stack de este repo levantado que bajar. Sin
   archivos temporales sueltos.

Working tree final (`git status --short`): `docs/architecture.md`,
`feature_list.json`, `progress/history.md`, `rabbitmq/README.md`
modificados; `progress/impl_observability.md`, `progress/review_observability.md`
y `tests/observability.rs` nuevos. `progress/current.md` sin diferencias
respecto a HEAD. Commit pendiente (no se hizo commit en esta sesión de
cierre, según protocolo del implementador).
