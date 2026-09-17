# Investigación: listener AMQPS (TLS, puerto 5671) en `rabbitmq:4.3.5-management` (imagen oficial)

> Alcance: solo investigación/documentación para la feature `tls`. No se ha
> tocado `rabbitmq/`, `docker-compose.yml`, `contracts/`, `tests/`, `src/`
> ni `Cargo.toml` — eso le corresponde al subagente `implementer`. Este
> documento reúne, con citas a fuentes oficiales, lo necesario para que esa
> implementación no tenga que volver a investigar nada.

Contexto de seguridad leído (`docs/security-scope.md`): el `ScanRequest`
lleva una credencial SSH real en `ssh_credentials_ref`; por eso TLS es un
requisito duro de punta a punta, el puerto 5672 en claro (si se deja) debe
quedar marcado explícitamente como solo-dev, nunca se deshabilita la
verificación TLS "para que funcione más rápido", y ningún certificado/clave
real de producción se commitea — solo material de laboratorio,
identificable como tal.

Estado actual del repo (verificado antes de investigar):
- `rabbitmq/rabbitmq.conf` ya existe y solo tiene `load_definitions =
  /etc/rabbitmq/definitions.json` (con un comentario explicando por qué se
  usa esa clave y no `management.load_definitions`, ver
  `progress/explore_definitions_format.md`). **Hay que añadir el bloque TLS
  a este mismo archivo, sin tocar esa línea.**
- `docker-compose.yml` monta `./rabbitmq/definitions.json` y
  `./rabbitmq/rabbitmq.conf` de forma read-only (`:ro`) y expone 5672 y
  15672, con un comentario explícito de que 5671/TLS se habilita en la
  feature `tls`.
- `rabbitmq/` no tiene aún ningún directorio de certificados.

---

## 1. Claves de `rabbitmq.conf` para el listener AMQPS

Fuente primaria: [TLS Support | RabbitMQ](https://www.rabbitmq.com/docs/ssl)
(doc oficial, sección "Enabling TLS"), contrastada con el fuente versionado
4.3 en GitHub:
[`rabbitmq-website/versioned_docs/version-4.3/ssl/index.md`](https://github.com/rabbitmq/rabbitmq-website/blob/main/versioned_docs/version-4.3/ssl/index.md).
Puertos: [Networking — RabbitMQ](https://www.rabbitmq.com/docs/networking#ports).

### 1.1 Listener

```ini
listeners.ssl.default = 5671
```

- `listeners.ssl.default` abre un listener AMQP 0-9-1/1.0 **con TLS** en el
  puerto indicado (convención: 5671, el puerto AMQPS estándar registrado
  ante IANA). Es el equivalente TLS de `listeners.tcp.default = 5672`.
- La doc también admite formas indexadas (`listeners.ssl.1 = ...`) para
  bindear a una interfaz concreta, p. ej. el ejemplo oficial `listeners.ssl.1
  = 192.168.1.99:5671 # TLS (AMQPS)`; para este repo, con un único listener
  en todas las interfaces del contenedor, `listeners.ssl.default` es
  suficiente y es la forma que usan los ejemplos "simple" de la doc.
- **Obligatoria** si se quiere el listener TLS activo. Sin ella, el nodo
  sigue arrancando pero no hay AMQPS.

### 1.2 Bloque `ssl_options.*`

```ini
ssl_options.cacertfile = /path/to/ca_certificate.pem
ssl_options.certfile   = /path/to/server_certificate.pem
ssl_options.keyfile    = /path/to/server_key.pem
```

| Clave | Qué hace | Obligatoria para TLS simple (server-only) |
|---|---|---|
| `ssl_options.cacertfile` | Ruta al bundle de CA con el que RabbitMQ valida el certificado del *peer* cuando se le pide verificarlo, y que también se puede necesitar para completar la cadena de confianza que el servidor presenta al cliente. | **Sí** — el proceso de RabbitMQ debe poder leer este archivo al arrancar aunque no se exija verificación de cliente, porque forma parte del material TLS que carga el listener. |
| `ssl_options.certfile` | Ruta al certificado público del servidor (el que RabbitMQ presenta al cliente en el handshake TLS). | **Sí** |
| `ssl_options.keyfile` | Ruta a la clave privada del servidor, pareja de `certfile`. | **Sí** |
| `ssl_options.verify` | Activa o no la verificación del certificado del *peer*. Valores: `verify_peer` (verifica) o `verify_none` (no verifica). | **Opcional** para TLS de transporte sin mTLS obligatorio. Con `verify_none`, el servidor no le exige ni valida certificado de cliente — el canal sigue cifrado igualmente, solo que sin autenticación mutua por certificado. |
| `ssl_options.fail_if_no_peer_cert` | Solo tiene efecto si `ssl_options.verify = verify_peer`. Con `true`, el handshake se rechaza si el cliente no presenta certificado; con `false` (o si `verify = verify_none`), se aceptan clientes que no presentan certificado. | **Opcional / irrelevante** cuando no se exige mTLS. La propia doc de RabbitMQ señala explícitamente que `fail_if_no_peer_cert = false` permite "accept clients which don't present a certificate" incluso con `verify_peer` — es decir, `verify_peer` sin `fail_if_no_peer_cert = true` **no** basta para forzar mTLS. |

Para este repo — el `security-scope.md` exige TLS de transporte pero **no**
exige verificación obligatoria de cliente (mTLS) — el bloque mínimo
correcto es:

```ini
listeners.ssl.default = 5671

ssl_options.cacertfile = /etc/rabbitmq/tls/ca_certificate.pem
ssl_options.certfile   = /etc/rabbitmq/tls/server_certificate.pem
ssl_options.keyfile    = /etc/rabbitmq/tls/server_key.pem
```

Sin `ssl_options.verify` / `ssl_options.fail_if_no_peer_cert`: RabbitMQ usa
por defecto `verify_none` (no exige ni valida certificado de cliente), que
es exactamente "TLS de transporte, sin mTLS obligatorio" — coherente con lo
que pide el alcance de esta feature. Si en el futuro se decide exigir mTLS
(decisión de diseño fuera de esta investigación), se añadiría
`ssl_options.verify = verify_peer` **y** `ssl_options.fail_if_no_peer_cert =
true` juntas — una sin la otra no basta, según el punto de arriba.

**Nota sobre rutas dentro del contenedor**: al ser la imagen oficial
`docker-library/rabbitmq` (no Bitnami), las rutas de `ssl_options.*` son
rutas de archivo normales *dentro del contenedor*; no hay variables de
entorno equivalentes desde RabbitMQ 3.9 — el Docker Hub oficial de la
imagen documenta explícitamente que `RABBITMQ_SSL_CACERTFILE`,
`RABBITMQ_SSL_CERTFILE`, `RABBITMQ_SSL_KEYFILE` y variables relacionadas
"no longer used" desde 3.9, y remite a usar `rabbitmq.conf` vía bind-mount /
Docker Configs / `COPY`, que es exactamente el patrón que ya usa este repo
para `definitions.json` y el propio `rabbitmq.conf` (fuente: [RabbitMQ —
Official Image | Docker
Hub](https://hub.docker.com/_/rabbitmq)). Esto confirma que el enfoque
correcto es montar los ficheros de certs como volumen y apuntar
`ssl_options.*` a esas rutas montadas, igual que ya se hace con
`load_definitions`.

---

## 2. Generar cert/clave autofirmados de laboratorio

Dos caminos documentados oficialmente:

### Opción A (recomendada por RabbitMQ): `tls-gen`

Fuente: [rabbitmq/tls-gen](https://github.com/rabbitmq/tls-gen) — "Generates
self-signed x509/TLS/SSL certificates useful for development", mantenido
por el propio equipo de RabbitMQ y referenciado desde la doc oficial de TLS.

```bash
git clone https://github.com/rabbitmq/tls-gen
cd tls-gen/basic
make CN=localhost
make alias-leaf-artifacts   # nombres de archivo "host-neutral"
```

- Genera CA + par de claves de servidor y de cliente con un solo comando
  (`make`), bajo `./result/`.
- El CN por defecto es el hostname del sistema; se sobreescribe con
  `make CN=<host>` — para este repo, el nombre de servicio en
  `docker-compose.yml` (`rabbitmq`) o `localhost` según desde dónde se
  conecte.
- Explícitamente documentado como "self-signed and only suitable for
  development and test environments" — coincide con lo que exige
  `docs/security-scope.md` (material de laboratorio, nunca de producción).
- Contra: añade una dependencia externa (clonar el repo `tls-gen`, tener
  `make`/Python 3.5+/openssl) solo para generar certs; para un repo que ya
  declara "no publica ninguna librería compartida" y prioriza
  reproducibilidad simple, puede ser más pesado de lo necesario.

### Opción B: `openssl` manual (CA propia + cert de servidor firmado por ella)

Es el patrón estándar (CA autofirmada → CSR de servidor → firma de la CA
sobre el CSR) que la propia doc de RabbitMQ describe a alto nivel ("Whether
the certificates are self-signed or issued by a trusted CA, they are
configured the same way. Required files include: CA certificate, server
certificate, server private key...", vía la doc de TLS Support). Es la
opción que da control total con una sola herramienta ya presente en
cualquier entorno de desarrollo Linux/CI, sin clonar un repo externo — más
alineado con "reproducible" y con no añadir dependencias fuera de lo que ya
usa el repo (`openssl`, disponible en la imagen `rabbitmq:*-management` y en
casi cualquier runner de CI).

Propuesta de script (a implementar como `rabbitmq/generate-lab-certs.sh` por
el `implementer` de la feature `tls`, revisado por el `reviewer` contra
`docs/security-scope.md`):

```bash
#!/usr/bin/env bash
# rabbitmq/generate-lab-certs.sh
#
# Genera una CA de laboratorio y un certificado de servidor firmado por
# ella, para habilitar AMQPS (TLS) en el rabbitmq:4.3.5-management local.
#
# NO usar en producción: CA y clave privada quedan en texto plano en
# rabbitmq/tls/, pensadas para ser regeneradas y/o descartadas, nunca
# commiteadas con datos reales (ver docs/security-scope.md).
#
# Uso: ./rabbitmq/generate-lab-certs.sh
set -euo pipefail

CERT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/tls"
DAYS=825          # tope aceptado por navegadores/clientes modernos para hojas
CA_DAYS=3650       # la CA de laboratorio puede vivir más
SUBJ_CA="/C=CL/O=security-app-lab/OU=broker/CN=security-app-lab-CA"
SUBJ_SERVER="/C=CL/O=security-app-lab/OU=broker/CN=rabbitmq"

mkdir -p "$CERT_DIR"
cd "$CERT_DIR"

# --- 1. CA de laboratorio ---------------------------------------------
openssl genrsa -out ca_key.pem 4096
openssl req -x509 -new -nodes \
  -key ca_key.pem \
  -sha256 -days "$CA_DAYS" \
  -subj "$SUBJ_CA" \
  -out ca_certificate.pem

# --- 2. Clave + CSR del servidor, con SAN para docker-compose ----------
# SAN incluye:
#   - rabbitmq   -> nombre del servicio en docker-compose.yml (DNS interno)
#   - localhost  -> conexiones desde el host (p. ej. management UI, tests
#                    de integración corridos fuera de la red de compose)
#   - 127.0.0.1  -> IP loopback explícita
cat > server_ext.cnf <<'EOF'
subjectAltName = DNS:rabbitmq,DNS:localhost,IP:127.0.0.1
extendedKeyUsage = serverAuth
EOF

openssl genrsa -out server_key.pem 4096
openssl req -new \
  -key server_key.pem \
  -subj "$SUBJ_SERVER" \
  -out server_request.csr

openssl x509 -req \
  -in server_request.csr \
  -CA ca_certificate.pem -CAkey ca_key.pem -CAcreateserial \
  -sha256 -days "$DAYS" \
  -extfile server_ext.cnf \
  -out server_certificate.pem

rm -f server_request.csr server_ext.cnf ca_certificate.srl

chmod 640 ca_key.pem server_key.pem
chmod 644 ca_certificate.pem server_certificate.pem

echo "Certificados de laboratorio generados en $CERT_DIR:"
echo "  ca_certificate.pem      (CA de laboratorio, pública)"
echo "  ca_key.pem              (clave privada de la CA de laboratorio)"
echo "  server_certificate.pem  (cert de servidor, firmado por la CA de arriba)"
echo "  server_key.pem          (clave privada del servidor)"
```

Notas de diseño del script (a validar por `reviewer`):
- SAN con `rabbitmq` (nombre del servicio en `docker-compose.yml`) +
  `localhost` + `127.0.0.1` cubre tanto las conexiones intra-red-de-compose
  (`ms-nmap`/`ms-analisis`/Gateway conectando a `rabbitmq:5671`) como
  conexiones desde el host para depuración manual — sin esto, clientes que
  validan el hostname del cert (`verify_peer` del lado *cliente*, o
  simplemente un cliente estricto) fallarían aunque el servidor esté bien
  configurado. Requiere `-extfile` porque `openssl req`/`x509 -req` no
  toman SAN por `-subj`.
- `CN=rabbitmq` en el subject del servidor (no `localhost`) porque es el
  nombre por el que los demás servicios del `docker-compose.yml` resuelven
  al broker; el SAN cubre los demás casos.
- Salida a `rabbitmq/tls/` — mismo patrón de "carpeta junto a
  `rabbitmq.conf`/`definitions.json`" que ya usa el repo, montable 1:1 en
  `docker-compose.yml`.
- El script debe quedar documentado (README de `rabbitmq/` o comentario en
  el propio script, ya incluido arriba) como generador de material
  **exclusivamente de laboratorio** — coherente con
  `docs/security-scope.md` ("nunca un certificado ni una clave privada real
  de producción commiteados en el repo"). Debe decidirse además (fuera de
  esta investigación) si `rabbitmq/tls/*.pem` se commitea (regenerable,
  claramente de laboratorio) o se añade a `.gitignore` y se documenta como
  paso de setup — ambas son defendibles, pero commitear una clave privada
  aunque sea de laboratorio suele evitarse por higiene; queda para que lo
  decida el `implementer`/`reviewer` de la feature `tls`, no esta
  investigación.

---

## 3. Montaje en `docker-compose.yml`

El compose actual ya monta `rabbitmq.conf` y `definitions.json` así:

```yaml
    volumes:
      - ./rabbitmq/definitions.json:/etc/rabbitmq/definitions.json:ro
      - ./rabbitmq/rabbitmq.conf:/etc/rabbitmq/rabbitmq.conf:ro
```

Análogamente, montando el directorio completo generado por el script de la
sección 2 (evita tener que listar cada `.pem` uno a uno y mantiene el mismo
patrón "una carpeta del repo → una ruta del contenedor"):

```yaml
services:
  rabbitmq:
    image: rabbitmq:4.3.5-management
    ports:
      - "5672:5672"   # AMQP en claro — solo desarrollo/depuración local
      - "5671:5671"   # AMQPS (TLS) — puerto real de conexión para servicios
      - "15672:15672" # UI de management, solo desarrollo local
    volumes:
      - ./rabbitmq/definitions.json:/etc/rabbitmq/definitions.json:ro
      - ./rabbitmq/rabbitmq.conf:/etc/rabbitmq/rabbitmq.conf:ro
      - ./rabbitmq/tls:/etc/rabbitmq/tls:ro
```

Y en `rabbitmq/rabbitmq.conf`, añadir bajo la línea `load_definitions`
existente (sin tocarla):

```ini
load_definitions = /etc/rabbitmq/definitions.json

listeners.ssl.default = 5671

ssl_options.cacertfile = /etc/rabbitmq/tls/ca_certificate.pem
ssl_options.certfile   = /etc/rabbitmq/tls/server_certificate.pem
ssl_options.keyfile    = /etc/rabbitmq/tls/server_key.pem
```

Montar el directorio `:ro` es coherente con cómo ya se montan
`definitions.json`/`rabbitmq.conf` en este repo, y evita que el proceso
dentro del contenedor pueda escribir sobre el material TLS.

**Importante para `testcontainers`** (los tests de integración de este
repo, ver `docs/verification.md`): `testcontainers` levanta su propio
contenedor efímero de `rabbitmq:4.3.5-management` — para que esos tests
verifiquen TLS igual que el compose local, el mismo `rabbitmq.conf` +
directorio `rabbitmq/tls/` deben montarse/copiarse también en la
configuración de `testcontainers` (fuera del alcance de esta investigación
puntual sobre AMQPS del compose, pero relevante para que la feature `tls`
quede completa — detalle de implementación de `implementer`).

---

## 4. ¿Deshabilitar 5672 en claro, o dejarlo documentado como solo-dev?

Clave para deshabilitarlo del todo (confirmada en
[TLS Support | RabbitMQ](https://www.rabbitmq.com/docs/ssl), sección sobre
listeners):

```ini
listeners.tcp = none
```

Con esto, el nodo **no** abre ningún listener AMQP en claro (ni 5672 ni
ningún otro) — solo queda accesible por `listeners.ssl.default` (5671, o el
puerto configurado). La doc lo describe explícitamente como forma de
"deactivate non-TLS listeners, only TLS-enabled clients will be able to
connect".

### Opción 1 — `listeners.tcp = none` (deshabilitar 5672 del todo)

- Implicaciones:
  - Fuerza que **toda** conexión (incluidos los tests de `testcontainers`,
    herramientas de depuración tipo `rabbitmqadmin`/CLI si no soportan TLS
    por defecto, etc.) use AMQPS — máxima coherencia con "TLS obligatorio
    en toda conexión" de `docs/security-scope.md`.
  - Simplifica la superficie: no hay que recordar "no exponer 5672 fuera
    del contenedor", porque directamente no existe.
  - Puede complicar depuración rápida local (p. ej. un `amqp-tools` o
    cliente que no maneja bien TLS autofirmado sin configuración extra) y
    obliga a que *todo* el tooling de desarrollo confíe en la CA de
    laboratorio.
  - Nota: la **UI de management** (puerto 15672, HTTP) es un listener
    aparte (`management.tcp.*` / `management.ssl.*`), no se ve afectada
    por `listeners.tcp = none` — esa clave solo controla los listeners AMQP
    core. Deshabilitar 5672 no deshabilita la UI de management en claro por
    sí solo; sería una decisión/clave separada si se quisiera TLS también
    ahí.

### Opción 2 — dejar 5672 abierto, documentado como solo-dev

- Es literalmente lo que ya dice el comentario actual del compose ("Puerto
  5672 (AMQP en claro): expuesto aquí solo para desarrollo/depuración
  local") y lo que pide `docs/security-scope.md`: *"Si por alguna razón se
  deja el puerto AMQP en claro (5672) accesible en el docker-compose.yml
  local, debe quedar explícitamente marcado como solo-desarrollo/depuración
  en la documentación y, si es posible, no expuesto fuera del propio
  contenedor."*
- Implicaciones:
  - Mantiene una vía de depuración simple (p. ej. `rabbitmqctl`,
    `rabbitmqadmin`, o un cliente AMQP simple sin configurar TLS) para
    desarrollo local.
  - Riesgo: si por error algún servicio real (`ms-nmap`, Gateway) se
    conectara a 5672 en vez de 5671, la credencial SSH viajaría en claro —
    exactamente el escenario que `security-scope.md` marca como el riesgo
    central. Mitigable definiendo el usuario/permiso igual en ambos
    listeners (RabbitMQ no permite restringir un usuario a un listener
    concreto de forma nativa) — es decir, la única barrera real contra ese
    error es *no* darle a los servicios ninguna razón/configuración para
    apuntar a 5672, y opcionalmente no publicar el puerto 5672 fuera de la
    red de docker-compose (quitar el mapeo `"5672:5672"` del host pero
    dejarlo accesible solo entre contenedores de la misma red) — el propio
    `security-scope.md` sugiere esto último ("si es posible, no expuesto
    fuera del propio contenedor").

**Esta investigación no decide entre las dos** — es una decisión de diseño
de la feature `tls` (a discutir `implementer`/`reviewer`/usuario si hace
falta, según el protocolo de `docs/security-scope.md` §"Si algo no está
claro"). Ambas claves (`listeners.tcp = none` vs. dejar
`listeners.tcp.default` implícito/5672 abierto) son válidas y están
documentadas arriba con sus citas.

---

## 5. Qué cambia para certificados reales de producción

A alto nivel (para documentación, no aplica a esta feature de laboratorio):

- Las claves de `rabbitmq.conf` (`listeners.ssl.default`,
  `ssl_options.cacertfile`, `ssl_options.certfile`, `ssl_options.keyfile`)
  **no cambian de forma ni de nombre** — RabbitMQ no distingue en su config
  si un certificado es autofirmado o emitido por una CA real/gestionada:
  la doc oficial lo dice explícitamente ("Whether the certificates are
  self-signed or issued by a trusted CA, they are configured the same
  way").
- Lo único que cambia es **el origen de los archivos** que esas tres claves
  apuntan:
  - `ssl_options.cacertfile` → el bundle de la CA pública real (o
    intermedia) que emitió el certificado de servidor, en vez de
    `ca_certificate.pem` generado por el script de laboratorio.
  - `ssl_options.certfile` / `ssl_options.keyfile` → el certificado y clave
    privada emitidos por esa CA real (p. ej. vía ACME/Let's Encrypt, una CA
    corporativa, o el servicio TLS gestionado del proveedor cloud —
    p. ej. un Secrets Manager/Key Vault que inyecta el material TLS como
    archivo montado o como *sidecar*), en vez de los generados por
    `rabbitmq/generate-lab-certs.sh`.
- La clave privada real **nunca** se commitea al repo (a diferencia del
  material de laboratorio, que sí puede documentarse/regenerarse
  reproduciblemente): en un despliegue real, `ssl_options.keyfile` apuntaría
  a una ruta montada desde un volumen/secret gestionado por la plataforma
  (Kubernetes Secret, Docker Swarm secret, servicio de gestión de
  certificados del proveedor cloud, etc.), nunca a un archivo versionado —
  mismo principio que ya aplica este repo a las contraseñas de usuarios de
  RabbitMQ (`docs/security-scope.md`: "su contraseña, en cualquier entorno
  real, se gestiona fuera del repo").
- Si se pasa a `verify_peer`/mTLS en algún momento (fuera del alcance
  actual), tampoco cambia la forma de las claves — solo se añaden
  `ssl_options.verify = verify_peer` y `ssl_options.fail_if_no_peer_cert =
  true`, como ya se documentó en la sección 1.
- Renovación/rotación de certificados reales (vencimiento, revocación) es
  un problema operacional que no tiene equivalente en el material de
  laboratorio (que se regenera a mano con el script); en producción typically
  se automatiza (cert-manager, ACME, rotación del proveedor cloud) — fuera
  del alcance de este repo de infraestructura de Broker per se, pero vale
  la pena que quede anotado para quien opere el despliegue real.

---

## Resumen accionable para el `implementer` de la feature `tls`

1. Crear `rabbitmq/generate-lab-certs.sh` (contenido propuesto en §2),
   ejecutarlo para generar `rabbitmq/tls/{ca_certificate,ca_key,
   server_certificate,server_key}.pem`, y decidir si se commitean o se
   documentan como paso de setup (`.gitignore`).
2. Añadir a `rabbitmq/rabbitmq.conf` (sin tocar `load_definitions`):
   `listeners.ssl.default = 5671` + el bloque `ssl_options.cacertfile /
   certfile / keyfile` apuntando a `/etc/rabbitmq/tls/...` (§1, §3).
3. Añadir a `docker-compose.yml`: puerto `"5671:5671"` y volumen
   `./rabbitmq/tls:/etc/rabbitmq/tls:ro` (§3).
4. Decidir y documentar (con el usuario si hace falta, según
   `docs/security-scope.md`) si 5672 se deshabilita del todo
   (`listeners.tcp = none`) o se deja documentado como solo-dev, e
   idealmente no publicado fuera de la red de compose (§4).
5. Propagar el mismo material TLS a la config de `testcontainers` para que
   los tests de integración verifiquen AMQPS real, no solo el compose local
   (mencionado en §3, detalle de implementación pendiente).
6. Documentar en `docs/architecture.md`/README de `rabbitmq/` la nota de
   producción de §5 (sin implementarla — es solo para quien despliegue
   fuera de este entorno de laboratorio).

## Fuentes citadas

- [TLS Support | RabbitMQ](https://www.rabbitmq.com/docs/ssl) — claves
  `listeners.ssl.*`, `ssl_options.*`, ejemplo de configuración, semántica
  de `verify`/`fail_if_no_peer_cert`, `listeners.tcp = none`.
- [rabbitmq-website versioned_docs 4.3 — ssl/index.md](https://github.com/rabbitmq/rabbitmq-website/blob/main/versioned_docs/version-4.3/ssl/index.md)
  — misma doc, fuente versionada para 4.3.x (coincide con el tag
  `4.3.5-management` pineado en este repo).
- [Networking — RabbitMQ](https://www.rabbitmq.com/docs/networking#ports)
  — puertos 5671 (AMQPS) / 5672 (AMQP), ejemplo `listeners.ssl.1 =
  192.168.1.99:5671 # TLS (AMQPS)`.
- [rabbitmq/tls-gen](https://github.com/rabbitmq/tls-gen) (GitHub, mantenido
  por el equipo de RabbitMQ) — generación de certs de laboratorio vía
  `make`, `CN=`, advertencia explícita "only suitable for development and
  test environments".
- [RabbitMQ — Official Image | Docker Hub](https://hub.docker.com/_/rabbitmq)
  — confirma que las variables `RABBITMQ_SSL_*` están deprecadas desde
  3.9 y que la vía soportada es `rabbitmq.conf` montado (mismo patrón que
  `load_definitions` ya usa este repo).
