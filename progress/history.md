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
