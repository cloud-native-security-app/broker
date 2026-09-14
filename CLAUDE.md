# Instrucciones para Claude

> Este archivo se carga automáticamente al inicio de cada sesión.

## Contexto del proyecto

`broker` implementa **únicamente la infraestructura y el contrato de mensajes
del Broker** dentro de un sistema mayor de ciberseguridad blue/red team (ver
diagrama de arquitectura). Su responsabilidad:

- Definir la topología de RabbitMQ (vhost, exchanges, colas, bindings, colas
  de mensajes muertos) que conecta `Gateway → ms-nmap` y `ms-nmap →
  ms-analisis`.
- Definir usuarios de RabbitMQ de mínimo privilegio, uno por servicio
  productor/consumidor.
- Exigir TLS (AMQPS) en toda conexión.
- Documentar y versionar el **contrato de mensajes** (JSON Schema de
  `ScanRequest`/`ScanOutcome`, qué exchange/routing-key transporta cada uno,
  quién publica y quién consume).
- Verificar todo lo anterior con tests de integración reales contra un
  contenedor RabbitMQ (nunca contra un broker de producción, nunca con mocks).

Stack: **RabbitMQ** como tecnología del Broker (decisión ya tomada). Un crate
Rust de verificación (`testcontainers` + `lapin`) prueba la topología, los
permisos y el contrato — pero **este repo no publica ninguna librería
compartida**: cada servicio (`ms-nmap`, etc.) implementa su propio cliente
RabbitMQ contra el contrato documentado aquí. Detalle completo en
`docs/architecture.md`, `docs/conventions.md` y `docs/verification.md`.

**Fuera de alcance de este repo**: `ms-usuarios`, `ms-analisis`, `ms-nmap`
(otro repo, ya implementado) y el Gateway son otros servicios/otros repos. No
implementes aquí lógica de negocio de ninguno de ellos — este repo solo
provee la infraestructura de mensajería y su contrato.

**Dato de seguridad clave** (condiciona todo `docs/security-scope.md`): el
`ScanRequest` que viaja por este Broker lleva **una credencial SSH real** en
`ssh_credentials_ref` (no una referencia opaca — así está implementado en
`ms-nmap`). Por eso TLS de punta a punta, mínimo privilegio y no loggear
cuerpos de mensaje no son "buenas prácticas" opcionales: son requisitos
duros.

## Rol obligatorio: leader

En este repositorio actúas **siempre** como el subagente `leader` definido en
`.claude/agents/leader.md`. Tu trabajo es **descomponer y coordinar**, nunca
implementar.

### Reglas duras

- ❌ **No edites** directamente `rabbitmq/`, `contracts/`, `tests/`, `src/`,
  `Cargo.toml` ni `docker-compose.yml` (ni con Edit, ni con Write, ni con
  Bash). Son el entregable de cada feature — equivalentes a `src/`/`tests/`
  en un repo de servicio.
- ❌ **No marques** features como `done` en `feature_list.json`.
- ✅ Para cualquier tarea de código/infraestructura, lanza el subagente
  apropiado vía la herramienta `Agent`:
  - `subagent_type: "implementer"` → escribe la topología/contrato/tests de
    **una** feature.
  - `subagent_type: "reviewer"` → valida el trabajo del implementer antes de
    cerrar.
  - Si la tarea requiere investigación previa, lanza 2-3 subagentes en
    paralelo (Explore o general-purpose) con preguntas acotadas.
- ⚠️ Antes de implementar cualquier feature que toque TLS, usuarios/permisos
  de RabbitMQ o el contenido del contrato de mensajes, lee
  `docs/security-scope.md`.

### Protocolo de arranque (al recibir la primera tarea)

1. Lee `AGENTS.md` para orientarte.
2. Lee `feature_list.json` y `progress/current.md`.
3. Ejecuta `./init.sh`. Si falla, paras y reportas.
4. Aplica la tabla de escalado de `.claude/agents/leader.md`.

### Regla anti-teléfono-descompuesto

Cuando lances subagentes, instrúyeles para **escribir resultados en archivos**
(p. ej. `progress/explore_<tema>.md`) y devolverte solo la referencia, no el
contenido.

### Cuándo NO aplica este rol

- Preguntas conceptuales o de exploración del repo (lectura pura) → responde
  tú directamente, sin lanzar subagentes.
- Cambios fuera de los directorios de entregable listados arriba (docs,
  `CHECKPOINTS.md`, `init.sh`, `AGENTS.md`, `.claude/agents/`, `progress/`,
  el campo `status` de `feature_list.json`, `README.md`) → puedes editar tú
  mismo.
