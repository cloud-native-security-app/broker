# Implementación: feature 5 — `cancellation_contract` (RF-14)

> Informe del `implementer`. No se marcó la feature como `done` en
> `feature_list.json` — pendiente del veredicto del `reviewer` y de que el
> `leader` cierre la sesión.

## Contexto de partida

Sesión ya iniciada por el `leader`: `feature_list.json` id 5 ya estaba en
`"in_progress"` y `progress/current.md` ya traía el plan completo (topología
+ retry acotado igual que feature 2, JSON Schema igual que feature 4,
permisos según `docs/security-scope.md` §"Cobertura de las features
añadidas en la ronda 2"). No hubo que investigar nada nuevo: todo el patrón
a replicar ya existía y estaba aprobado en el repo.

## Archivos modificados

- `rabbitmq/definitions.json`:
  - Nuevo exchange `scan.cancellations` (`topic`) y su dead-letter
    `scan.cancellations.dlx` (`direct`), mismo patrón que
    `scan.requests`/`scan.requests.dlx`.
  - Nueva cola `ms-nmap.scan-cancellations` (quorum, `x-delivery-limit: 3`,
    `x-dead-letter-exchange: scan.cancellations.dlx`,
    `x-dead-letter-routing-key: ms-nmap.scan-cancellations.dead`) — mismo
    N=3 ya justificado en `rabbitmq/README.md` para la feature 2, sin
    reinventarlo. Más su `ms-nmap.scan-cancellations.dlq`.
  - Bindings: `scan.cancellations` → `ms-nmap.scan-cancellations` con
    routing key `scan.cancellation`; `scan.cancellations.dlx` →
    `ms-nmap.scan-cancellations.dlq` con `ms-nmap.scan-cancellations.dead`.
  - Permisos ampliados exactamente como pedía el brief: `gateway.write`
    pasó de `^scan\.requests$` a `^scan\.(requests|cancellations)$`;
    `ms-nmap.read` pasó de `^ms-nmap\.scan-requests$` a
    `^ms-nmap\.(scan-requests|scan-cancellations)$`. `ms-analisis` no se
    tocó (ni sus permisos ni ninguna otra parte de la topología de
    features 2/4).
- `contracts/scan-cancellation.schema.json` (nuevo): draft 2020-12,
  `correlation_id`+`requested_by` como únicos campos, ambos `required`,
  `additionalProperties: false`. Sin campos extra (ni `schema_version` ni
  ningún dato del escaneo) — deliberadamente mínimo, como pide el brief.
- `contracts/README.md`: nueva fila en la tabla de archivos, nueva sección
  `## ScanCancellation` (exchange/routing-key/publicador/consumidor/campos)
  y una subsección explícita "Consumo y cancelación real — trabajo
  PENDIENTE en `nmap-service`", citando que
  `nmap-service/src/messaging/consumer.rs` (leído, no editado) solo
  implementa `ScanRequestSource` para `ScanRequest` — ningún tipo/trait
  para leer `ms-nmap.scan-cancellations` ni para abortar un escaneo en
  curso existe todavía ahí.
- `rabbitmq/README.md`: tabla de exchanges/colas y tabla de permisos
  actualizadas con la nueva fila/columnas de `scan.cancellations`, para
  mantener la doc de topología (que ya cubría features 2/3) en sync con
  `definitions.json` — no era un criterio de aceptación explícito de la
  feature 5 pero sí una consecuencia directa de "no hardcodear topología
  por duplicado sin que coincida con la fuente de verdad" (`docs/
  conventions.md`).
- `src/contracts.rs`: `SCAN_CANCELLATION_SCHEMA` (`include_str!`),
  `scan_cancellation_validator()` (mismo patrón `OnceLock` que los otros
  dos), `validate_scan_cancellation()` pública, y 4 tests unitarios nuevos
  en el `mod tests` existente:
  - `valid_scan_cancellation_passes`
  - `scan_cancellation_missing_correlation_id_is_rejected`
  - `scan_cancellation_missing_requested_by_is_rejected`
  - `scan_cancellation_with_unknown_extra_field_is_rejected` (verifica
    específicamente que un intento de colar `ssh_credentials_ref` en este
    mensaje es rechazado — refuerza la regla de seguridad "este mensaje
    nunca lleva una credencial").
- `tests/topology_exists.rs`: actualizado para reflejar la topología
  ampliada (ahora 6 exchanges, 8 colas, 9 bindings) — **necesario**, no
  opcional: el test hacía comparación exacta de conjuntos
  (`assert_eq!(queue_names, expected_queues)` etc.) y habría fallado en
  rojo con la nueva cola/exchange presentes pero no declarados en las
  listas `expected_*`. No se tocó ninguna aserción sobre la topología de
  las features 2/4, solo se añadieron las entradas nuevas.
- `tests/cancellation.rs` (nuevo): 3 tests de integración
  `#[ignore = "requiere Docker"]`, reutilizando `tests/common/mod.rs` sin
  modificarlo:
  - `gateway_can_publish_cancellation_and_it_arrives_at_ms_nmap_queue`
    (positivo end-to-end: valida el payload contra el schema, publica como
    `gateway`, confirma la entrega íntegra en `ms-nmap.scan-cancellations`
    leyendo como `lab-admin` — igual patrón que
    `tests/permissions.rs::gateway_can_write_scan_requests_and_read_own_outcomes`,
    donde gateway nunca lee la cola de otro servicio, ni siquiera la que
    él mismo alimentó).
  - `ms_analisis_cannot_write_or_read_cancellation_channel` (negativo:
    `ms-analisis` no puede publicar en `scan.cancellations` ni leer
    `ms-nmap.scan-cancellations`, verificado con `is_amqp_soft_error()`).
  - `ms_nmap_cannot_write_to_cancellation_exchange` (negativo adicional,
    no exigido literalmente por el acceptance pero coherente con "ningún
    otro usuario tiene acceso": `ms-nmap` solo consume la cancelación, su
    permiso de escritura sigue acotado a `scan.outcomes`).
- `progress/current.md`: bitácora actualizada en tiempo real con cada paso
  (no solo al final), como exige `AGENTS.md` §3.

## Decisiones de diseño dentro del margen del brief

- Reutilicé exactamente el mecanismo de retry de la feature 2
  (`x-delivery-limit: 3`, cola quorum, sin TTL intermedio) tal como pedía
  el brief — no evalué alternativas.
- El exchange de dead-letter es `direct` (no `topic`), igual criterio que
  `scan.requests.dlx`/`scan.outcomes.dlx` (documentado en
  `rabbitmq/README.md`: solo necesita enrutar hacia un destino fijo por
  routing key exacta).
- Añadí un tercer test negativo (`ms-nmap` no puede escribir) que no estaba
  literalmente en el acceptance list (que solo pedía el caso negativo de
  "`ms-analisis` u otro") pero refuerza el mismo criterio de mínimo
  privilegio sin ampliar el alcance de la feature — es una aserción
  adicional sobre el mismo canal, no una feature nueva.
- Actualicé `rabbitmq/README.md` (tablas de topología y permisos) aunque no
  estaba en el acceptance list explícito de la feature 5, porque dejarlo
  desactualizado violaría la convención de que la documentación de
  topología no puede divergir de `definitions.json`. Es un cambio de
  alcance mínimo (solo añade filas/entradas, no reescribe nada existente).

## Verificación

`./init.sh` terminó en verde:

- `cargo fmt --check`: sin diferencias.
- `cargo clippy --all-targets -- -D warnings`: sin warnings.
- `cargo test` (sin Docker): **15/15** tests unitarios pasan, incluidos los
  4 nuevos de `validate_scan_cancellation`.
- `cargo test -- --ignored` (con Docker real vía `testcontainers`):
  **17/17** tests de integración pasan, incluidos los 3 nuevos de
  `tests/cancellation.rs` y los 14 preexistentes de las features 2-4
  (`message_contract.rs` ×2, `outcome_routing.rs` ×2, `permissions.rs` ×6,
  `retry_delivery_limit.rs` ×1, `tls.rs` ×2, `topology_exists.rs` ×1 — este
  último con las aserciones ampliadas a la nueva topología).
- `cargo doc`: sin errores.
- `docker compose config`: válido (no se tocó `docker-compose.yml`, ya
  monta `rabbitmq/definitions.json` completo).

Un primer intento de `tests/cancellation.rs` falló en rojo
(`ACCESS_REFUSED` leyendo `ms-nmap.scan-cancellations` como `gateway`) por
un error mío: intenté confirmar la entrega leyendo con el propio canal de
`gateway`, que correctamente no tiene permiso de lectura sobre esa cola.
Corregido usando el canal de `lab-admin` para la verificación de entrega
(mismo patrón ya usado en `tests/permissions.rs` y
`tests/message_contract.rs`), sin tocar los permisos de `gateway`.

## Reglas de seguridad

- El mensaje de cancelación solo lleva `correlation_id`+`requested_by`;
  ningún campo de credencial. Verificado explícitamente por el test unitario
  `scan_cancellation_with_unknown_extra_field_is_rejected`.
- No se amplió el permiso de `ms-analisis` en ningún sentido.
- No se tocó TLS ni ningún exchange/cola/permiso de las features 2/3/4 salvo
  las dos ampliaciones de regex pedidas explícitamente.
- Ningún payload de ejemplo/fixture usa credenciales reales — solo valores
  de laboratorio (`req-2026-0042`, `analyst@example.test`), consistentes con
  los ya usados en `src/contracts.rs`/`tests/message_contract.rs`.

## Pendientes / bloqueos

Ninguno. La feature queda lista para el `reviewer`; el trabajo de que
`ms-nmap` realmente consuma y honre la cancelación sigue, como estaba
previsto desde el inicio, fuera de alcance de este repo (documentado en
`contracts/README.md` y `rabbitmq/README.md`).

## Cierre de sesión (2026-09-17)

El `reviewer` emitió veredicto **APPROVED sin cambios requeridos**
(`progress/review_cancellation_contract.md`): verificó permisos, topología,
schema y ambos tests (unitarios e integración) contra Docker real, y
confirmó independientemente contra
`nmap-service/src/messaging/consumer.rs` que la nota de "trabajo pendiente"
es honesta.

Pasos de cierre ejecutados siguiendo `AGENTS.md` §5:

1. `./init.sh` re-ejecutado de punta a punta: exit code 0,
   **15/15** tests unitarios y **17/17** tests de integración
   (`--ignored`, contra `rabbitmq:management` real vía `testcontainers`)
   en verde, incluidos los 3 nuevos de `tests/cancellation.rs` y los 14
   preexistentes de las features 2-4. `cargo fmt --check`/`clippy -D
   warnings`/`doc` y `docker compose config` limpios.
2. `feature_list.json`: id 5 (`cancellation_contract`) →
   `status: "done"`.
3. Resumen de `progress/current.md` movido al final de
   `progress/history.md` (sección `## 2026-09-17 — Feature 5
   \`cancellation_contract\` — DONE`).
4. `progress/current.md` vaciado, solo queda la plantilla original.
5. Verificado `docker ps -a`/`docker compose ps -a`: sin contenedores ni
   servicios de este repo levantados (los contenedores presentes en el
   host pertenecen a otros proyectos ajenos, todos detenidos). No se
   levantó ningún stack manual en esta sesión de cierre — `testcontainers`
   gestionó y limpió sus propios contenedores durante `cargo test
   -- --ignored`. Sin archivos `*.tmp` sueltos. No se tocó nada de
   `nmap-service` (solo lectura, ya confirmado por el `reviewer`).

No se modificó la implementación ya aprobada: `./init.sh` no reveló ningún
problema nuevo en esta re-ejecución.
