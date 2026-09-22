# Review — feature 7 (`deployment_docs`)

**Veredicto:** APPROVED

## Verificación de los 8 puntos pedidos

1. **Carga de `rabbitmq/definitions.json` en una instancia real.**
   `README.md` §"Cargar la topología en una instancia real" (líneas ~33-64
   del diff) menciona explícitamente las 3 vías exigidas por el acceptance
   de la feature 7: `POST /api/definitions` (API de management), `rabbitmqctl
   import_definitions <archivo>`, y "el mecanismo del proveedor
   cloud/plataforma elegido — *a definir según el proveedor que se decida*".
   Este tercer punto NO nombra AWS/GCP/Azure como decisión tomada; las únicas
   menciones de esos proveedores en todo el diff (`docs/architecture.md:204`
   y `README.md:65,67`) son explícitamente negativas ("no se asume",
   "congelaría una decisión que no le corresponde tomar a este repo"), y el
   único ejemplo positivo dado (un operador de Kubernetes) está marcado
   textualmente como "solo como ejemplo ilustrativo y no como decisión
   tomada". Cumple.

2. **Qué cambia respecto al compose local.** `README.md` §"Qué cambia
   respecto al `docker-compose.yml` local" cubre los 4 puntos exigidos por
   el acceptance: certificados TLS reales (referenciando
   `rabbitmq/README.md` §"Qué cambia para certificados reales de
   producción", verificado que existe en la línea 248 de ese archivo),
   contraseñas de servicio gestionadas fuera del repo (Vault/Secrets
   Manager/Key Vault/Kubernetes Secret, nunca hardcodeadas), UI de
   management no expuesta públicamente (restringida a red privada/VPN/
   bastión/proxy autenticado), y monitoreo conectado a un sistema de
   alertas real (exporter/scraper sobre los endpoints de `observability`
   alimentando "el sistema de alertas real de la organización", sin
   inventar un proveedor concreto). Los 4 puntos están en términos
   genéricos como exige el acceptance. Cumple.

3. **Configuración por servicio.** Confirmado directamente en
   `nmap-service/README.md` líneas 60-61 (leído en esta revisión) que las
   2 variables reales que expone `ms-nmap` para el Broker son, letra por
   letra, `MS_NMAP_BROKER_ENDPOINT` y `MS_NMAP_BROKER_CREDENTIAL` — el
   nombre y la descripción coinciden exactamente con lo que documenta
   `README.md` de este repo. Para `gateway`/`ms-analisis`, el nuevo texto
   no inventa ningún nombre de variable de entorno concreto: solo habla en
   términos genéricos de host, vhost (`security-app`), usuario ya definido
   en `rabbitmq/definitions.json` y "credencial obtenida siempre de fuera
   de este repo". Cumple.

4. **Ninguna credencial real.** `git diff README.md docs/architecture.md |
   grep -iE "password|secret|credential"` solo devuelve líneas que hablan
   de *mecanismos* (gestor de secretos, `password_hash` como concepto,
   nombres de variables) — ningún valor concreto de contraseña, hash o
   secreto. Cumple.

5. **Sin código ni tests nuevos.** `git diff --stat` muestra únicamente
   `README.md` (+103/-0), `docs/architecture.md` (+69/-0),
   `feature_list.json` (+1/-1, solo el campo `status`) y `progress/current.md`
   (+44/-0). `git status --porcelain` confirma que no hay cambios en
   `src/`, `tests/`, `Cargo.toml`, `rabbitmq/definitions.json` ni
   `docker-compose.yml`. Verificado también que `tests/` en el working tree
   es idéntico al de la última feature `observability` (`git ls-tree -r
   aa5570b -- tests/` vs `git status` actual: sin diferencias). Cumple.

6. **`./init.sh` en verde, mismo número de tests.** Ejecutado directamente:
   exit code 0. `cargo fmt --check` sin diferencias, `cargo clippy` sin
   warnings, `docker compose config` válido. Conteo de tests: **15
   unitarios** (`ok`, 0 failed) y **20 de integración** marcados
   `#[ignore = "requiere Docker"]`, todos corriendo verdes contra Docker
   real: `cancellation.rs` (3) + `message_contract.rs` (2) +
   `observability.rs` (3) + `outcome_routing.rs` (2) + `permissions.rs` (6)
   + `retry_delivery_limit.rs` (1) + `tls.rs` (2) + `topology_exists.rs`
   (1) = 20. Coincide exactamente con el estado dejado por la feature
   `observability` (mismos archivos de test, sin adiciones ni
   eliminaciones). Cumple.

   **Hallazgo menor (no bloqueante):** `progress/impl_deployment_docs.md`
   (línea 68) afirma "18 tests de integración... (mismo conteo que
   antes)". El conteo real, verificado por mí, es **20**, no 18 — el
   implementer se equivocó al contar en su propio informe, aunque el
   número real de tests es correcto y no cambió respecto a `observability`.
   Es una imprecisión en el texto del informe, no en el entregable
   (`README.md`/`docs/architecture.md`), así que no amerita
   `CHANGES_REQUESTED`, pero se deja registrado.

7. **"Feature futura potencial" no implementada.** `docs/architecture.md`
   §"Trabajo futuro potencial" documenta dos ideas (test de
   `rabbitmqctl import_definitions` end-to-end; reemplazar el placeholder
   de proveedor cloud) explícitamente como NO implementadas. No hay
   ningún archivo nuevo en `tests/`/`src/` relacionado. `git diff
   feature_list.json` confirma que el único cambio es `status: "pending"`
   → `"in_progress"` en la feature 7 — no se agregó ninguna feature 8 ni
   se tocó ninguna otra entrada. Cumple.

8. **Calidad/exactitud técnica de las referencias cruzadas.** Verificado
   contra `rabbitmq/README.md`:
   - "Credenciales de laboratorio" existe (línea 145) y su contenido sobre
     `password_hash` coincide con lo que `README.md` resume.
   - "Qué cambia para certificados reales de producción" existe (línea
     248) bajo "TLS (AMQPS, feature `tls`)" y coincide con lo resumido en
     el nuevo texto (origen de `cacertfile`/`certfile`/`keyfile`, gestor de
     secretos, sin cambiar la forma de `rabbitmq.conf`).
   - "Observabilidad" existe (línea 278) y coincide con lo que
     `docs/architecture.md` dice sobre RNF-09/RNF-10 y los endpoints usados.
   - "Puerto 5672 (AMQP en claro)" existe (línea 235), referenciado
     correctamente como solo-desarrollo.
   No se detectó ninguna contradicción entre lo que esta feature afirma y
   lo que las features `tls`/`observability`/`topology_definition` ya
   documentan/implementan. Cumple.

## Checkpoints (CHECKPOINTS.md, estado tras esta feature)

- C1 (arnés completo): [x] — los 4 archivos base y los 4 docs existen;
  `./init.sh` exit 0.
- C2 (estado coherente): [x] — solo la feature 7 está `in_progress` (el
  resto `done`); `progress/current.md` describe la sesión activa, sin
  basura de sesiones previas.
- C3 (arquitectura respetada): [x] — no se crearon directorios nuevos; no
  se tocó `rabbitmq/definitions.json` (sigue sin credenciales reales); no
  se tocaron permisos, `contracts/`, ni `Cargo.toml`; no hay código nuevo
  que pueda violar el resto de reglas de C3 (no se agregó código).
- C4 (verificación real): [x] — sin cambios en `tests/`; 20 tests de
  integración contra `rabbitmq:management` real vía `testcontainers`
  siguen pasando; `cargo clippy --all-targets -- ...` limpio (vía
  `init.sh`); `docker compose config` válido.
- C5 (sesión cerrada bien): [ ] ← Razón: no bloqueante para esta feature en
  sí, pero `progress/current.md` todavía no tiene la bitácora de cierre de
  esta sesión ni se movió a `progress/history.md`; eso le corresponde al
  leader después de este veredicto, no es un defecto del trabajo del
  implementer sobre `deployment_docs`.

## Cambios requeridos

Ninguno bloqueante. Sugerencia no bloqueante: corregir el conteo "18" por
"20" en `progress/impl_deployment_docs.md` si se quiere que el informe sea
exacto (no afecta al entregable real).
