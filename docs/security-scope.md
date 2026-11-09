# Alcance de seguridad y autorización

> `broker` provee la infraestructura de mensajería por la que circulan
> **credenciales SSH reales** (dentro de `ScanRequest.ssh_credentials_ref`,
> ver `docs/architecture.md` §"Contrato de mensajes") entre `Gateway`,
> `ms-nmap` y `ms-analisis`. Este documento define los límites duros que
> aplican tanto al desarrollo (tests, ejemplos, LocalStack local) como
> al diseño de la topología. No es un documento legal — es la guía
> práctica que un agente debe seguir antes de tocar TLS, usuarios/permisos
> o el contenido del contrato de mensajes.

## El hecho central: hay secretos reales en tránsito

- `ScanRequest.ssh_credentials_ref` es la credencial SSH **real** con la
  que `ms-nmap` se autentica en el objetivo — no una referencia opaca a un
  vault. Así está implementado y testeado en `ms-nmap`
  (`src/domain.rs::SshCredentialsRef`).
- Esto significa que **cualquier cosa que pueda leer el cuerpo de un
  mensaje en `scan-requests` (o en su cola `ms-nmap-scan-requests` / DLQ) puede leer una credencial SSH activa**. Todo
  el diseño de este repo (cifrado, transporte, permisos, retención) parte de esa premisa.
- Si en el futuro se decide que el Broker deje de transportar la
  credencial real (p. ej. sustituirla por una referencia a un vault
  resuelta por `ms-nmap` directamente), es un cambio de contrato que se
  discute explícitamente con el usuario y probablemente empieza por
  `ms-nmap`, no por este repo — no se improvisa aquí.

## Transporte y cifrado obligatorios

- Toda conexión a SNS/SQS contra AWS real va por HTTPS. Las políticas de
  tópicos y colas niegan `aws:SecureTransport=false` (Terraform en `infra`,
  `terraform/messaging.tf`; `tests/topology_parity.rs` comprueba que la
  condición sigue declarada).
- Las colas (y las DLQ) usan cifrado en reposo SSE-SQS con claves gestionadas
  por AWS (decisión Q4, sin KMS).
- El HTTP en claro solo existe entre los tests y el LocalStack efímero de
  `testcontainers`; nunca contra AWS real.
- Nunca se deshabilita la verificación TLS en el cliente "para que funcione
  más rápido" — reabre el riesgo de que la credencial SSH viaje en claro.

## Permisos: mínimo privilegio acotado por el Learner Lab

- En el Learner Lab todas las tareas ECS usan el mismo `LabRole`; no hay
  usuarios por servicio (compromiso aceptado el 2026-10-17, ver
  `docs/decisions.md` D-13). El aislamiento que sí existe son las políticas de
  recurso: cada cola solo acepta `sqs:SendMessage` desde SNS con
  `aws:SourceArn` igual al tópico al que está suscrita, y hay un tópico y una
  cola por flujo.
- Una cola no se comparte entre servicios: cada consumidor tiene la suya con
  su propia DLQ.
- La consola/credenciales de administración de AWS se gestionan fuera del
  repo, nunca hardcodeadas.

## Desarrollo y tests

- **Nunca** se usan credenciales reales (de AWS o del contrato de
  mensajes — p. ej. un `ssh_credentials_ref` real) en tests, fixtures o
  ejemplos. Solo valores de
  laboratorio, claramente ficticios.
- Los tests de integración corren contra un LocalStack real levantado por
  `testcontainers` (credenciales `test`/`test`), contenedor desechable —
  nunca contra una cuenta AWS de staging o producción.
- Los payloads de ejemplo usados para probar el contrato (incluidos los
  que llevan un `ssh_credentials_ref` de prueba) deben quedar
  identificables como ficticios a simple vista (p. ej.
  `"lab-only-not-a-real-secret"`), para que nadie los confunda con un
  secreto real si se filtran a un log.

## Retención y logging

- Las colas no retienen mensajes más tiempo del necesario para su
  propósito operativo: `ms-nmap-scan-requests` conserva 1 h y su DLQ 24 h
  (llevan credenciales SSH); el resto usa los valores por defecto de SQS. Las
  copias en DLQ persisten la credencial hasta que vence la retención.
- Ningún test, script o configuración de este repo debe loggear el cuerpo
  completo de un mensaje real (con `ssh_credentials_ref`) a stdout/stderr
  de forma persistente; para depuración puntual en desarrollo, usar
  siempre payloads de laboratorio (ver arriba).

## Alcance de autorización

- Este repo **no** implementa autenticación ni autorización de usuarios
  finales — eso es responsabilidad de `Gateway`/`ms-usuarios`. La
  "autorización" que sí modela este repo es a nivel de **servicio**
  (qué microservicio puede publicar/consumir en qué tópico/cola), no de
  persona.
- Este repo no valida que el `ScanRequest` que transporta esté autorizado
  para su objetivo — esa verificación ya se asume hecha aguas arriba
  (Gateway/`ms-usuarios`), igual que documenta `ms-nmap` en su propio
  `docs/security-scope.md`.

## Cobertura de ScanCancellation

`ScanCancellation` lleva `correlation_id` y `requested_by`, **nunca** una
credencial; aplican las mismas reglas de transporte, cifrado y aislamiento
por recurso de arriba.

## Si algo no está claro

Si una feature planificada en `odd/tasks/` roza alguno de estos límites y no
está claro cómo proceder, el agente **para y pregunta al usuario** en vez
de asumir qué está autorizado — igual que cualquier otro bloqueo, se
documenta en el feature doc correspondiente de `odd/tasks/`.
