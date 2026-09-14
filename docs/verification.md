# Verificación — Cómo demostrar que el trabajo funciona

> Regla de oro: **el agente no dice "funciona", lo demuestra**.
> Toda feature termina con evidencia ejecutable, no con afirmaciones.

## Niveles de verificación

### Nivel 0 — Artefactos estructuralmente válidos (obligatorio)

```bash
docker compose config          # si existe docker-compose.yml
```

Y, para cada JSON Schema en `contracts/` y para `rabbitmq/definitions.json`:
son JSON válido y, en el caso de los schemas, JSON Schema válido (se puede
verificar con un test Rust que los cargue con el crate `jsonschema`, no hace
falta una herramienta externa).

### Nivel 1 — Tests unitarios (obligatorio si hay lógica pura)

Cualquier función pura del crate de verificación (p. ej. validación de un
payload contra un schema) tiene al menos un test que cubre el camino feliz
y un camino de error.

```bash
cargo test
```

### Nivel 2 — Lints y formato (obligatorio)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Ningún warning de clippy se ignora en silencio; si es un falso positivo, se
documenta con `#[allow(...)]` y un comentario explicando por qué.

### Nivel 3 — Tests de integración (obligatorio desde la feature `topology_definition`)

- Contra un contenedor `rabbitmq:<tag>-management` real, levantado con
  `testcontainers` en el propio test. **Nunca** contra una instancia de
  RabbitMQ de staging/producción.
- Se marcan `#[ignore = "requiere Docker"]` (ver `docs/conventions.md`), se
  ejecutan con `cargo test -- --ignored`, y requieren Docker disponible. Si
  falla por falta de Docker, se documenta como bloqueo en
  `progress/current.md` — no se reemplaza por un mock.
- Cubren, como mínimo: la topología existe tal cual `definitions.json` la
  declara; cada usuario de servicio puede hacer **solo** lo suyo (positivo
  + negativo); el dead-lettering funciona; TLS es exigible (una conexión
  sin TLS se rechaza, o la limitación de la imagen usada para probarlo
  queda documentada).

### Nivel 4 — Smoke test end-to-end del contrato (obligatorio para `message_contract`)

Publica un `ScanRequest` de ejemplo (con un `ssh_credentials_ref` de
laboratorio, nunca real) en el exchange correspondiente y verifica que
llega íntegro a la cola de `ms-nmap`; publica un `ScanOutcome` de ejemplo
(caso `completed` y caso `failed`) y verifica que llega a la cola de
`ms-analisis`. Ambos payloads deben además validar contra su JSON Schema de
`contracts/`.

## Anti-patrones (no hacer)

- ❌ "Definí la topología, debería funcionar." → falta test ejecutable
  contra un RabbitMQ real.
- ❌ Test que solo verifica que la conexión no falla. → tiene que
  comprobar el contenido concreto (existencia exacta de exchange/cola/
  binding, permisos exactos).
- ❌ Probar contra una instancia de RabbitMQ de staging o producción, o con
  credenciales reales de ningún tipo.
- ❌ Silenciar un warning de `clippy` con `#[allow(...)]` sin comentario.
- ❌ Marcar la feature como `done` sin pasar `./init.sh`.
- ❌ Dar por buena una topología donde un usuario tiene más permisos de los
  que su servicio necesita "por si acaso".

## Verificación final antes de cerrar

```bash
./init.sh           # debe terminar con [OK] Entorno listo
```

Si `./init.sh` está rojo, **no** marques nada como `done`. Anota el bloqueo
en `progress/current.md` y pon `"status": "blocked"` en `feature_list.json`.
