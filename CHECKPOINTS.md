# CHECKPOINTS — Evaluación del estado final

> En sistemas multi-agente no se evalúa el camino, se evalúa el destino.
> Estos son los checkpoints objetivos que un juez (humano o IA) puede usar
> para decidir si el proyecto está sano.

## C1 — El arnés está completo

- [ ] Existen los 4 archivos base: `AGENTS.md`, `init.sh`, `feature_list.json`,
      `progress/current.md`.
- [ ] Existen los 4 docs: `docs/architecture.md`, `docs/conventions.md`,
      `docs/verification.md`, `docs/security-scope.md`.
- [ ] `./init.sh` termina con exit code 0.

## C2 — El estado es coherente

- [ ] Como mucho una feature en `in_progress` en `feature_list.json`.
- [ ] Toda feature `done` tiene tests asociados que pasan.
- [ ] `progress/current.md` está vacío o describe la sesión activa
      (no contiene basura de sesiones anteriores).

## C3 — El trabajo respeta la arquitectura

- [ ] Solo existen los directorios previstos en `docs/architecture.md`
      (`docker-compose.yml`, `rabbitmq/`, `contracts/`, `src/`, `tests/`).
- [ ] `rabbitmq/definitions.json` no contiene credenciales reales ni de
      producción (solo valores de laboratorio).
- [ ] Cada usuario de RabbitMQ tiene permisos acotados solo a lo que su
      servicio necesita (sin acceso "por si acaso" a colas/exchanges
      ajenos).
- [ ] `contracts/` coincide con la forma real que `ms-nmap` ya implementa
      (no hay campos inventados sin acordar, p. ej. `schema_version`).
- [ ] Toda dependencia en `Cargo.toml` (si existe) está justificada por una
      feature de `feature_list.json`.
- [ ] No hay `println!`/`dbg!` sueltos para debug, ni `unwrap()`/`panic!()`
      fuera de tests sin justificar, ni TODOs sin contexto.

## C4 — La verificación es real

- [ ] `tests/` tiene al menos un test de integración contra un
      `rabbitmq:management` real (vía `testcontainers`), nunca contra una
      instancia de staging/producción ni con mocks.
- [ ] Los tests de permisos cubren el caso positivo (un usuario puede lo
      suyo) y el caso negativo (no puede lo de otro servicio).
- [ ] `cargo test` (si existe `Cargo.toml`) muestra > 0 tests y todos
      verdes; `cargo clippy --all-targets -- -D warnings` no muestra
      advertencias.
- [ ] `docker compose config` (si existe `docker-compose.yml`) valida sin
      error.

## C5 — La sesión se cerró bien

- [ ] No hay archivos sin trackear sospechosos (`*.tmp`, `target/` fuera del
      `.gitignore`, contenedores/certificados de prueba olvidados).
- [ ] `progress/history.md` tiene una entrada por la última sesión.
- [ ] La última feature trabajada está reflejada en su estado correcto.

---

**Cómo usar este archivo:** un agente revisor (`.claude/agents/reviewer.md`)
recorre cada checkbox, marca `[x]` o `[ ]`, y rechaza el cierre de sesión
si quedan boxes vacíos en C1-C5.
