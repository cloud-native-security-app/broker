# Informe de implementación — feature `topology_definition` (id 2)

Fecha: 2026-09-14/15. Implementer (subagente), sesión única.

## Resumen

Implementada la topología real de RabbitMQ que conecta Gateway → ms-nmap →
ms-analisis/Gateway, con reintentos acotados (RNF-06) y usuarios de mínimo
privilegio, verificada con 10 tests de integración reales (`testcontainers`
+ `lapin`, contra `rabbitmq:4.3.5-management`) — todos en verde, `./init.sh`
completo (incluido `--ignored`, con Docker real) termina en `[OK] Entorno
listo`.

## Archivos creados

- **`rabbitmq/definitions.json`** (nuevo): vhost `security-app`; exchanges
  `scan.requests`/`scan.outcomes` (topic) con sus DLX `scan.requests.dlx`/
  `scan.outcomes.dlx` (direct); colas `ms-nmap.scan-requests`,
  `ms-analisis.scan-outcomes`, `gateway.scan-outcomes` (todas `quorum`,
  `x-delivery-limit: 3`) con sus `.dlq`; bindings exactos, incluidos los
  diferenciados de `scan.outcomes` (`ms-analisis.scan-outcomes` solo
  `scan.outcome.completed`/`scan.outcome.failed`, `gateway.scan-outcomes`
  con `scan.outcome.#`); 4 usuarios (`lab-admin` + los 3 de servicio) con
  `password_hash` de laboratorio (SHA-256+salt, algoritmo verificado contra
  el vector de ejemplo oficial de `rabbitmq.com/docs/passwords` antes de
  generar los hashes reales) y permisos `configure`/`write`/`read`
  acotados por regex exactamente como pide la feature.
- **`rabbitmq/rabbitmq.conf`** (nuevo): `load_definitions =
  /etc/rabbitmq/definitions.json` (sin el prefijo `management.`, por la
  razón documentada en `progress/explore_definitions_format.md` §0 e issue
  docker-library/rabbitmq#428).
- **`tests/common/mod.rs`** (nuevo): helpers compartidos —
  `start_broker()` (arranca `testcontainers_modules::rabbitmq::RabbitMq`
  con tag `4.3.5-management` y ambos archivos de `rabbitmq/` montados vía
  `ImageExt`/`Mount`), `connect_as()`, `amqp_uri()`,
  `management_base_url()`.
- **`tests/topology_exists.rs`** (nuevo, 1 test): verifica contra la API de
  management (autenticada como `lab-admin`) que exchanges, colas (con sus
  `arguments` exactos: `x-queue-type`, `x-delivery-limit`,
  `x-dead-letter-exchange`, `x-dead-letter-routing-key`) y los 7 bindings
  coinciden exactamente con `definitions.json`, incluida la ausencia
  explícita de un binding `ms-analisis.scan-outcomes`/`scan.outcome.started`.
- **`tests/permissions.rs`** (nuevo, 6 tests): un test positivo + uno
  negativo por cada uno de los 3 usuarios de servicio. Los casos positivos
  son end-to-end reales (se publica y se confirma la llegada del mensaje,
  no solo "no dio error"); los negativos verifican `403 ACCESS_REFUSED`
  (`lapin::Error::is_amqp_soft_error()`) al intentar leer/escribir un
  recurso ajeno.
- **`tests/outcome_routing.rs`** (nuevo, 2 tests): `scan.outcome.started`
  llega solo a `gateway.scan-outcomes`; `scan.outcome.completed` llega a
  ambas colas.
- **`tests/retry_delivery_limit.rs`** (nuevo, 1 test): agota
  `x-delivery-limit=3` con `basic.reject(requeue=true)` repetidos y
  verifica que el mensaje aparece en `ms-nmap.scan-requests.dlq` con
  `x-death[].reason == "delivery_limit"`, y que pasó por más de una
  entrega (no cayó directo a la `.dlq`).

## Archivos modificados

- **`docker-compose.yml`**: monta `rabbitmq/definitions.json` y
  `rabbitmq/rabbitmq.conf`; **se eliminaron** las variables de entorno
  `RABBITMQ_DEFAULT_USER`/`RABBITMQ_DEFAULT_PASS` (ver "Decisión de diseño
  no trivial" abajo) — las mismas credenciales de `lab-admin` ahora viven
  en `definitions.json`.
- **`Cargo.toml`**: añadidas las dependencias del brief —
  `testcontainers = "0.27"` (pineada a `0.27`, no `0.28`, por conflicto de
  resolución con `testcontainers-modules 0.15` — ver más abajo),
  `testcontainers-modules` (feature `rabbitmq`), `lapin`, `tokio`
  (`rt-multi-thread`, `macros`, `time`), `reqwest` (`json`), `serde`
  (`derive`), `serde_json`, `futures`. `Cargo.lock` regenerado y commiteado
  junto (383 paquetes).
- **`src/lib.rs`**: añadido el módulo público `lab_credentials` (usuario y
  contraseña de laboratorio de `lab-admin` + los 3 usuarios de servicio) y
  la constante `VHOST`. Es lo único público del crate — sigue sin
  binario/lógica de negocio, solo lo que los tests necesitan.
- **`rabbitmq/README.md`**: documentación completa de la topología,
  justificación de `x-delivery-limit=3` (con la nota empírica de
  `basic.reject` vs `basic.nack`, ver abajo), tabla de permisos, tabla de
  credenciales de laboratorio y la explicación del cambio de
  `RABBITMQ_DEFAULT_USER` a `definitions.json`.
- **`progress/current.md`**: plan del implementer añadido bajo el plan del
  leader (no se borró nada de lo escrito por el leader/explorers).

## Decisiones de diseño no triviales (y desviaciones documentadas del brief)

1. **`RABBITMQ_DEFAULT_USER`/`RABBITMQ_DEFAULT_PASS` eliminados de
   `docker-compose.yml`.** El brief no lo pedía explícitamente, pero
   mantenerlos junto a un `definitions.json` que también declara el mismo
   usuario `lab-admin` crea una fuente de verdad duplicada con orden de
   aplicación no documentado por RabbitMQ (¿se aplica el env var antes o
   después del import de definiciones en el primer boot?). Se optó por que
   `definitions.json` sea la única fuente de verdad de usuarios/permisos
   (coherente con la decisión de arquitectura "la topología se declara de
   forma imperativa en definitions.json"), manteniendo las mismas
   credenciales (`lab-admin` / `lab-only-not-a-real-secret`) para no
   romper flujos de desarrollo previos. Documentado en `rabbitmq/README.md`
   y en el comentario de `docker-compose.yml`.

2. **`testcontainers` pineado a `0.27`, no `0.28`.** `testcontainers-modules
   0.15.0` (única versión publicada con el módulo `rabbitmq`) depende de
   `testcontainers ^0.27`; forzar `0.28` produce un conflicto irresoluble
   de `bollard-stubs` en `cargo generate-lockfile`. Resuelto bajando a
   `0.27`, verificado con `cargo build --all-targets` limpio.

3. **Verificación empírica de `x-delivery-limit` cambió el mecanismo de
   nack de la investigación previa (desviación de
   `progress/explore_retry_pattern.md`).** La investigación asumía
   `basic.nack(requeue=true)` como mecanismo de simulación de fallo. Contra
   el contenedor real (`rabbitmq:4.3.5-management`) se comprobó
   empíricamente (ver metodología abajo) que **`basic.nack` nunca
   incrementa el contador `x-delivery-count`** que RabbitMQ compara contra
   `x-delivery-limit` — solo incrementa un contador informativo separado,
   `x-acquired-count`, y el mensaje nunca cae en la `.dlq` (probado hasta 8
   redeliveries sin corte). **`basic.reject`** (el método AMQP 0-9-1
   estándar de un solo mensaje, distinto de la extensión RabbitMQ
   `basic.nack`) sí incrementa `x-delivery-count` y dispara el
   dead-lettering exactamente en el límite configurado. Se confirmó de
   forma cruzada contra la API HTTP de management
   (`ackmode: "reject_requeue_true"`, que internamente también usa
   `basic.reject`). El test final (`tests/retry_delivery_limit.rs`) usa
   `basic.reject`, y la nota queda documentada en el propio archivo de test
   y en `rabbitmq/README.md`, para que un consumidor real (`ms-nmap`, etc.)
   no tropiece con lo mismo.
   - Metodología: se escribió un test auxiliar de depuración (no
     commiteado, eliminado antes de cerrar la sesión) que imprimía, en cada
     iteración, si el mensaje seguía en la cola de trabajo, sus headers
     (`x-acquired-count`/`x-delivery-count`) y si ya había aparecido en la
     `.dlq`; y en paralelo se reprodujo el mismo ciclo con `curl` directo
     contra `POST /api/queues/<vhost>/<queue>/get` de la API de management,
     que sí mostró `x-delivery-count` incrementando y el corte exacto en 3.
     Esa comparación aisló que el problema era el método AMQP usado
     (`nack` vs `reject`), no la configuración de la cola.
   - El test final usa un sondeo (`poll_get`, con reintentos cortos y un
     timeout de 3s) en vez de una sola llamada `basic.get`, porque
     inmediatamente tras el arranque del contenedor (elección de líder Raft
     reciente de la cola quorum) se observaron un par de `None` transitorios
     — el sondeo lo hace robusto; se corrió 4 veces seguidas sin fallos
     antes de darlo por bueno.

4. **Contraseñas de servicio distintas por usuario** (no una sola
   contraseña de laboratorio compartida): `lab-only-not-a-real-secret-
   gateway`, `-ms-nmap`, `-ms-analisis`. Facilita detectar en un log/test
   qué usuario se estaba usando, sin aportar ningún secreto real.

5. **`password_hash` calculado fuera del crate Rust** (no se añadió
   `sha2`/`base64` a `Cargo.toml`): se generaron los 4 hashes con un script
   Python de un solo uso (no commiteado) que implementa el algoritmo
   documentado (`rabbitmq.com/docs/passwords`: salt de 4 bytes + SHA-256 +
   base64), verificado primero contra el vector de ejemplo oficial de esa
   misma página antes de generar los hashes reales. Se prefirió esto a
   añadir una dependencia de cifrado al crate de verificación solo para
   generar valores estáticos una vez — la corrección del hash igual quedó
   demostrada al primer arranque del contenedor real (login exitoso de los
   4 usuarios, verificado con `curl`/`whoami` antes de escribir los tests
   Rust, y de nuevo por los propios tests de `tests/permissions.rs`).

## Resultado de `./init.sh` (última ejecución, con Docker real)

```
── 1. Verificando entorno ────────────────────────────── [OK] x3
── 2. Verificando archivos base del arnés ────────────── [OK] x8
── 3. Validando feature_list.json ──────────────────────  [OK] (7 features, 1 in_progress)
── 4. Validando docker-compose.yml ──────────────────────  [OK] docker compose config válido
── 5. Compilando, probando y documentando ───────────────
  [OK] cargo fmt --check sin diferencias
  [OK] cargo clippy --all-targets -D warnings sin warnings
  [OK] cargo test (0 unit tests; 10 tests de integración `ignored` correctamente)
  [OK] cargo test -- --ignored → 10/10 PASS (con Docker real):
       - topology_exists: 1/1
       - permissions: 6/6
       - outcome_routing: 2/2
       - retry_delivery_limit: 1/1
  [OK] cargo doc --no-deps sin errores
── 6. Resumen ────────────────────────────────────────── [OK] Entorno listo.
```

No quedaron contenedores Docker huérfanos (`testcontainers` limpia los
suyos al terminar el proceso; el `docker compose up`/`down` manual usado
para depurar el hash de contraseñas y el mecanismo de `x-delivery-limit`
se bajó con `docker compose down -v` antes de escribir el resto de tests).

## Cobertura exacta de los criterios de aceptación de `feature_list.json`

Los 11 criterios de la feature (vhost+exchanges+DLX; cola ms-nmap con su
DLQ; cola ms-analisis solo completed/failed; cola gateway con `#`;
reintentos limitados documentados y justificados; 3 usuarios con permisos
exactos; docker-compose carga definitions.json; test de topología exacta;
test de permisos positivo+negativo para los 3 usuarios; test de
enrutamiento started/completed; test de agotamiento de reintentos con
verificación de que pasó por el ciclo; `./init.sh` en verde) están
cubiertos — ver mapeo 1:1 arriba entre cada archivo/test y el criterio
correspondiente.

## Pendiente / fuera de alcance de esta sesión

- No se tocó TLS (feature `tls`, id 3, sigue `pending`) — las conexiones de
  los tests son AMQP en claro, coherente con que la feature `tls` es
  posterior en el roadmap.
- No se tocó `contracts/` (feature `message_contract`, id 4) — los
  payloads usados en los tests de esta feature son JSON de laboratorio
  mínimos (`{"correlation_id": "..."}`, etc.), no los schemas reales.
- No se marcó la feature como `done` en `feature_list.json` (sigue
  `in_progress`) — corresponde al líder tras el veredicto del `reviewer`.

## Bloqueos

Ninguno. Docker estuvo disponible durante toda la sesión y se usó
activamente para verificar cada test contra un broker real antes de darlo
por bueno (incluida la investigación empírica del punto 3 de arriba).

---

## Cierre de sesión (2026-09-14)

- **Veredicto del reviewer:** APPROVED sin cambios bloqueantes. Detalle
  completo en `progress/review_topology_definition.md`. Única observación
  no bloqueante: la dependencia `futures = "0.3.34"` en `Cargo.toml` no se
  usaba en ningún archivo del crate.
- **Corrección aplicada:** eliminada `futures = "0.3.34"` de
  `Cargo.toml`. Antes de quitarla se confirmó con `grep -rn "futures"
  tests/ src/ Cargo.toml` que la única aparición era la propia línea de la
  dependencia (ningún uso real en `src/` ni `tests/`), así que se optó por
  la opción por defecto (eliminar) en vez de documentar una reserva para
  uso futuro. `Cargo.lock` se regeneró solo al recompilar en la siguiente
  ejecución de `./init.sh`.
- **Re-verificación tras el cambio:** `./init.sh` ejecutado de nuevo
  completo (incluye `--ignored`, con Docker real disponible): `cargo fmt
  --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
  `cargo test -- --ignored` (10/10 tests de integración en verde:
  `topology_exists` 1/1, `permissions` 6/6, `outcome_routing` 2/2,
  `retry_delivery_limit` 1/1), `cargo doc --no-deps` — resultado final
  `[OK] Entorno listo.` sin ninguna regresión tras quitar `futures`.
- **Estado de `feature_list.json`:** id 2 (`topology_definition`) →
  `status: "done"`.
- **`progress/current.md` / `progress/history.md`:** el resumen de la
  sesión se movió al final de `progress/history.md` (entrada `## 2026-09-14
  — Feature 2 topology_definition — DONE`); `progress/current.md` quedó
  vacío con solo la plantilla original (sin feature en curso).
- **Limpieza de entorno Docker:** `docker compose ps -a` en este repo no
  muestra ningún servicio levantado (no había stack de `docker-compose.yml`
  de esta sesión corriendo al cierre); `docker ps -a` no muestra ningún
  contenedor RabbitMQ/testcontainers huérfano — los contenedores de
  `testcontainers` se limpian solos al terminar cada test, confirmado tras
  la re-ejecución de `./init.sh`. Los contenedores y volúmenes Docker
  preexistentes listados por `docker ps -a`/`docker volume ls`
  (`mongo:7`, `hannah-coffee`, volúmenes anónimos sin nombre reconocible)
  pertenecen a otros proyectos ajenos a este repo y no se tocaron, ya que
  no hay evidencia de que provengan de esta sesión (ningún contenedor
  RabbitMQ presente, ni activo ni detenido).
- **Archivos temporales:** ninguno pendiente; `git status --short` solo
  muestra los artefactos esperados de esta feature (más el ajuste puntual
  de `Cargo.toml`/`Cargo.lock` de este cierre).
