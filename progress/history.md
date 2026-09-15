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
