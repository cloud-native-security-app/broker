# Alcance de seguridad y autorización

> `broker` provee la infraestructura de mensajería por la que circulan
> **credenciales SSH reales** (dentro de `ScanRequest.ssh_credentials_ref`,
> ver `docs/architecture.md` §"Contrato de mensajes") entre `Gateway`,
> `ms-nmap` y `ms-analisis`. Este documento define los límites duros que
> aplican tanto al desarrollo (tests, ejemplos, `docker-compose.yml`) como
> al diseño de la topología. No es un documento legal — es la guía
> práctica que un agente debe seguir antes de tocar TLS, usuarios/permisos
> o el contenido del contrato de mensajes.

## El hecho central: hay secretos reales en tránsito

- `ScanRequest.ssh_credentials_ref` es la credencial SSH **real** con la
  que `ms-nmap` se autentica en el objetivo — no una referencia opaca a un
  vault. Así está implementado y testeado en `ms-nmap`
  (`src/domain.rs::SshCredentialsRef`).
- Esto significa que **cualquier cosa que pueda leer el cuerpo de un
  mensaje en `scan.requests` puede leer una credencial SSH activa**. Todo
  el diseño de este repo (TLS, permisos, retención) parte de esa premisa.
- Si en el futuro se decide que el Broker deje de transportar la
  credencial real (p. ej. sustituirla por una referencia a un vault
  resuelta por `ms-nmap` directamente), es un cambio de contrato que se
  discute explícitamente con el usuario y probablemente empieza por
  `ms-nmap`, no por este repo — no se improvisa aquí.

## TLS obligatorio

- Toda conexión a RabbitMQ (de cualquier servicio, y de los tests de este
  repo) debe usar AMQPS (TLS), no AMQP en claro.
- El `docker-compose.yml` de desarrollo local puede usar un certificado
  autofirmado, generado o documentado como parte de la feature `tls` —
  nunca un certificado ni una clave privada real de producción commiteados
  en el repo.
- Si por alguna razón se deja el puerto AMQP en claro (5672) accesible en
  el `docker-compose.yml` local, debe quedar **explícitamente marcado**
  como solo-desarrollo/depuración en la documentación y, si es posible, no
  expuesto fuera del propio contenedor.
- Nunca se deshabilita la verificación TLS "para que funcione más rápido"
  en desarrollo — eso reabre el riesgo de que la credencial SSH viaje en
  claro.

## Usuarios y permisos de mínimo privilegio

- Un usuario de RabbitMQ por servicio productor/consumidor (`gateway`,
  `ms-nmap`, `ms-analisis`), nunca un usuario compartido "genérico".
- Los permisos (`configure`/`write`/`read`) de cada usuario se acotan por
  regex **solo** a los exchanges/colas que su servicio necesita. Un
  `reviewer` debe rechazar cualquier topología donde un usuario tenga
  acceso a la cola de otro servicio.
- El usuario administrador de RabbitMQ (con acceso a la UI de management y
  a `definitions.json` completo) es distinto de los usuarios de servicio y
  su contraseña, en cualquier entorno real, se gestiona fuera del repo
  (variable de entorno o gestor de secretos) — nunca hardcodeada.
- La UI de management no se expone públicamente en un despliegue real; en
  local (`docker-compose.yml`) es aceptable para desarrollo, documentado
  como tal.

## Desarrollo y tests

- **Nunca** se usan credenciales reales (de RabbitMQ, TLS, o del contrato
  de mensajes — p. ej. un `ssh_credentials_ref` real) en tests, fixtures,
  `definitions.json` de ejemplo o `docker-compose.yml`. Solo valores de
  laboratorio, claramente ficticios.
- Los tests de integración corren contra un `rabbitmq:management` real
  levantado por `testcontainers`, contenedor desechable — nunca contra una
  instancia de staging o producción.
- Los payloads de ejemplo usados para probar el contrato (incluidos los
  que llevan un `ssh_credentials_ref` de prueba) deben quedar
  identificables como ficticios a simple vista (p. ej.
  `"lab-only-not-a-real-secret"`), para que nadie los confunda con un
  secreto real si se filtran a un log.

## Retención y logging

- Las colas no retienen mensajes más tiempo del necesario para su
  propósito operativo (procesar y hacer ack) — no se configuran como
  almacenamiento de largo plazo de mensajes que contienen credenciales.
- Ningún test, script o configuración de este repo debe loggear el cuerpo
  completo de un mensaje real (con `ssh_credentials_ref`) a stdout/stderr
  de forma persistente; para depuración puntual en desarrollo, usar
  siempre payloads de laboratorio (ver arriba).

## Alcance de autorización

- Este repo **no** implementa autenticación ni autorización de usuarios
  finales — eso es responsabilidad de `Gateway`/`ms-usuarios`. La
  "autorización" que sí modela este repo es a nivel de **servicio**
  (qué microservicio puede publicar/consumir en qué exchange/cola), no de
  persona.
- Este repo no valida que el `ScanRequest` que transporta esté autorizado
  para su objetivo — esa verificación ya se asume hecha aguas arriba
  (Gateway/`ms-usuarios`), igual que documenta `ms-nmap` en su propio
  `docs/security-scope.md`.

## Cobertura de las features añadidas en la ronda 2 (cancelación, observabilidad)

- **`ScanCancellation`** (feature `cancellation_contract`, RF-14): lleva
  `correlation_id` y `requested_by`, **nunca** una credencial — las mismas
  reglas de TLS y mínimo privilegio de arriba aplican sin cambios. El
  usuario `gateway` gana permiso de escritura sobre `scan.cancellations`;
  ningún otro usuario tiene acceso.
- **Observabilidad** (feature `observability`, RNF-09/RNF-10): los
  endpoints de la API de management usados (`/api/healthchecks/node`,
  `/api/queues/...`) exponen **conteos y metadatos** (profundidad de cola,
  número de consumidores), nunca el cuerpo de los mensajes — no hay riesgo
  adicional de fuga de la credencial SSH por esta vía. Aun así, la UI/API
  de management sigue sin exponerse públicamente (ver "Usuarios y permisos
  de mínimo privilegio" arriba).

## Si algo no está claro

Si una feature de `feature_list.json` roza alguno de estos límites y no
está claro cómo proceder, el agente **para y pregunta al usuario** en vez
de asumir qué está autorizado — igual que cualquier otro bloqueo, se
documenta en `progress/current.md`.
