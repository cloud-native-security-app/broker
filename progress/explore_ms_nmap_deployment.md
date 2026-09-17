# Investigación: cómo documenta `ms-nmap` su propio despliegue (para contrastar el estilo/contenido)

> Hecha por el leader directamente (lectura del repo hermano
> `/home/o-aguirre/Documents/duoc/cloud-native/security-app/nmap-service`),
> para que el `implementer` de `deployment_docs` documente el despliegue del
> Broker de forma análoga, sin inventar convenciones nuevas.

## 1. Estilo de `ms-nmap` (README.md líneas 23-62, docs/architecture.md líneas 181-211)

- `README.md` tiene una sección `## Despliegue (Docker)` corta y práctica:
  qué contiene la imagen, qué NO incluye, cómo construir/ejecutar, y una
  **tabla de variables de entorno** con su descripción.
- `docs/architecture.md` tiene una sección `## Despliegue` más detallada
  (por qué cada decisión — stage builder/runtime, usuario no-root, pin de
  imágenes por digest, etc.), complementaria a la del README, no
  duplicada.

## 2. Variables de entorno REALES que `ms-nmap` ya usa para el Broker

Confirmado en `nmap-service/README.md` línea 60-61 (tabla de variables de
entorno obligatorias):

| Variable | Descripción (tal cual la documenta `ms-nmap`) |
|---|---|
| `MS_NMAP_BROKER_ENDPOINT` | Endpoint del Broker de mensajería del que se consumen solicitudes y al que se publican resultados |
| `MS_NMAP_BROKER_CREDENTIAL` | Credencial/token de autenticación contra el Broker (secreto) |

**Importante**: hoy son genéricas ("endpoint"/"credential") porque, según
`nmap-service/src/messaging/publisher.rs` (docstring), la tecnología
concreta del broker "todavía no está decidida" desde el punto de vista de
`ms-nmap` (usa el trait `ScanResultSink`, no un cliente RabbitMQ concreto
todavía). Para la documentación de despliegue de `broker` (esta feature),
lo correcto es:
- Referenciar estas 2 variables ya existentes en `ms-nmap` como el punto de
  configuración real que ese servicio expone.
- Mapear qué debería contener cada una una vez que RabbitMQ (ya decidido en
  `broker`) sea la tecnología concreta: `MS_NMAP_BROKER_ENDPOINT` → la URI
  AMQPS completa (`amqps://<host>:5671/<vhost>`), `MS_NMAP_BROKER_CREDENTIAL`
  → la contraseña del usuario `ms-nmap` (nunca el valor, solo cómo se
  inyecta — gestor de secretos, no hardcodeada).
- NO inventar nombres de variables de entorno para `gateway`/`ms-analisis`
  que no existen en ningún repo real — documentar el patrón (host, vhost,
  usuario, credencial fuera del repo) en términos genéricos para esos dos
  servicios, ya que `ms-usuarios`/Gateway/`ms-analisis` no son parte de
  este plan (fuera de alcance, ver `CLAUDE.md`/`AGENTS.md`).

## 3. Qué no se puede confirmar desde este repo

- No hay decisión de proveedor cloud — la propia feature 7 dice
  explícitamente "sin asumir un proveedor si no se ha decidido". No
  inventar un proveedor concreto (AWS/GCP/Azure) ni nombres de servicios
  gestionados específicos salvo como ejemplos ilustrativos claramente
  marcados como tales.
- `gateway`/`ms-analisis` no tienen repo visible desde aquí con variables de
  entorno reales — documentar su configuración en términos genéricos
  (host, vhost, usuario, referencia a gestor de secretos), no inventar
  nombres de variable específicos para ellos.
