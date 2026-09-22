# Implementación: feature 7 — `deployment_docs`

## Archivos modificados (solo documentación, sin código/tests)

- `README.md`: nueva sección `## Despliegue` (después de `## Desarrollo`),
  con el mismo nivel práctico/breve que la sección `## Despliegue (Docker)`
  de `nmap-service/README.md`. Contiene:
  - "Cargar la topología en una instancia real": las 3 vías exigidas por
    el acceptance (API de management `POST /api/definitions`, referenciando
    `rabbitmq/README.md`; `rabbitmqctl import_definitions <archivo>`; el
    mecanismo del proveedor cloud/plataforma, marcado explícitamente como
    "a definir según el proveedor que se decida", sin nombrar AWS/GCP/Azure
    como decisión tomada — solo se menciona un operador de Kubernetes como
    ejemplo ilustrativo, etiquetado como tal).
  - "Qué cambia respecto al `docker-compose.yml` local": los 4 puntos
    exigidos (certificados TLS reales, contraseñas de servicio fuera del
    repo, UI de management no pública, monitoreo → alertas reales), cada
    uno enlazando/resumiendo la sección correspondiente ya existente en
    `rabbitmq/README.md` (features `tls` y `observability`) en vez de
    reescribirla.
  - "Cómo cada servicio apunta a la instancia real": tabla con las 2
    variables reales de `ms-nmap` (`MS_NMAP_BROKER_ENDPOINT`,
    `MS_NMAP_BROKER_CREDENTIAL`, tal cual confirmadas en
    `progress/explore_ms_nmap_deployment.md` / `nmap-service/README.md`,
    sin inventar nombres nuevos) y su mapeo a URI AMQPS / contraseña del
    usuario `ms-nmap`; para `gateway`/`ms-analisis`, descripción genérica
    (host, vhost `security-app`, usuario ya definido en
    `rabbitmq/definitions.json`, credencial fuera del repo) sin inventar
    variables de entorno específicas para ellos, siguiendo la instrucción
    del leader de no asumir nombres que no se puedan verificar.
  - Nota explícita en cada punto de que las credenciales nunca viven en
    este repo (referencia a `docs/security-scope.md`).

- `docs/architecture.md`: nueva sección `## Despliegue (feature
  deployment_docs)` (insertada entre `## Manejo de errores` y `## Qué NO
  hacer`, mismo lugar relativo que la sección `## Despliegue` de
  `nmap-service/docs/architecture.md`), con el nivel de detalle/justificación
  ("por qué", no "cómo") que exige `nmap-service` como referencia de estilo.
  Contiene: por qué no se asume un proveedor cloud, por qué el
  `docker-compose.yml` local no es el despliegue real (con la justificación
  de por qué los 4 cambios no son opcionales, ligados a la fuga de la
  credencial SSH real y a que RNF-09/RNF-10 requieren alertas reales, no
  solo endpoints que respondan bien), y por qué cada servicio se documenta
  con distinto nivel de certeza (ms-nmap con variables reales confirmadas,
  gateway/ms-analisis en términos genéricos).

## Decisiones de estilo/ubicación (dentro del margen dejado por el leader)

- Seguí el patrón exacto observado en `ms-nmap`: `README.md` para lo
  práctico/breve (comandos, tabla de variables), `docs/architecture.md`
  para el detalle/justificación, sin duplicar contenido entre ambos — cada
  uno referencia al otro.
- No creé una sección nueva en `rabbitmq/README.md`: esa feature
  (`observability`/`tls`) ya documentó las partes que esta feature debía
  referenciar (`POST /api/definitions`, "Qué cambia para certificados
  reales de producción", "Credenciales de laboratorio", "Observabilidad"),
  así que solo enlacé/resumí en vez de reescribir, tal como pedía el brief.
- El mecanismo del proveedor cloud se documentó como punto 3 de "Cargar la
  topología", explícitamente sin decisión tomada, con un ejemplo
  ilustrativo (operador de Kubernetes) marcado como tal — no se nombró
  ningún proveedor concreto (AWS/GCP/Azure) como si fuera la decisión.

## Resultado de `./init.sh`

Verde, exactamente igual que tras la feature `observability` (id 6): 7
features en `feature_list.json`, `cargo fmt --check` sin diferencias,
`cargo clippy` sin warnings, 15 tests unitarios (`ok`), **20** tests de
integración marcados `#[ignore = "requiere Docker"]` que corren y pasan
todos en la sección de Docker (mismo conteo que antes: no se agregó ni
quitó ningún test — corregido tras el veredicto del reviewer, que detectó
que aquí decía "18" por error de conteo mío; el número real, verificado
también por el reviewer, es 20), `cargo doc` sin errores. No hubo que tocar
`src/`, `tests/`, `rabbitmq/`, `contracts/`, `docker-compose.yml` ni
`Cargo.toml` — solo `README.md` y `docs/architecture.md`.

## "Feature futura potencial" detectada pero NO implementada

Documentada también dentro de `docs/architecture.md` §"Trabajo futuro
potencial" (para que quede visible en el propio repo, sin agregarla a
`feature_list.json` — esa decisión le corresponde al leader/usuario):

- Un test de integración que ejercite explícitamente
  `rabbitmqctl import_definitions` como mecanismo de carga (contra un nodo
  ya corriendo, no vía montaje de archivo al arrancar el contenedor como
  hacen los tests actuales de `topology_definition`). Sería una forma más
  directa de verificar que el mecanismo documentado en esta feature
  funciona de punta a punta, pero implica un test nuevo — fuera de alcance
  de `deployment_docs` (puramente documental) y no se agrega aquí.
- Reemplazar el placeholder genérico del proveedor cloud/plataforma por el
  mecanismo real, una vez el usuario decida una plataforma de despliegue
  concreta.

## Estado de la feature

`feature_list.json` id 7 sigue en `status: "in_progress"` (no lo cambié a
`done` — corresponde al leader tras el veredicto del `reviewer`, según el
protocolo).

## Cierre de sesión (post-veredicto APPROVED)

- Veredicto del reviewer: **APPROVED**, sin cambios bloqueantes
  (`progress/review_deployment_docs.md`), con un único hallazgo menor no
  bloqueante: el conteo "18 tests de integración" en la sección anterior de
  este informe era incorrecto — el conteo real, verificado también por el
  reviewer, es **20** (mismo entregable correcto, solo el número en el
  informe estaba mal). Corregido arriba.
- `./init.sh` re-ejecutado una vez más antes de cerrar: verde de punta a
  punta, **15 tests unitarios + 20 tests de integración** contra Docker
  real, sin cambios respecto al estado dejado por `observability` (id 6).
- `feature_list.json` id 7 → `status: "done"`. Confirmado que las 7
  features del roadmap quedan en `"done"` (ids 1-7).
- Resumen movido al final de `progress/history.md` (entrada "2026-09-17 —
  Feature 7 `deployment_docs` — DONE (última del roadmap)").
- `progress/current.md` vaciado, dejando solo la plantilla original.
- Verificado que no quedan archivos temporales sueltos ni contenedores/
  volúmenes Docker huérfanos de este repo: `docker compose ps -a` no
  muestra ningún servicio levantado; `docker ps -a` solo muestra
  contenedores de otros proyectos ajenos, ya existentes antes de esta
  sesión (no se levantó ningún stack de este repo en esta sesión de
  cierre, solo se corrió `./init.sh`, cuyos tests de integración usan
  `testcontainers` y se limpian solos).
- **Fecha de cierre:** 2026-09-17. Esta era la última feature del roadmap
  original de 7 features — cualquier trabajo futuro requeriría acordar
  explícitamente una feature nueva en `feature_list.json` antes de
  implementarse.
