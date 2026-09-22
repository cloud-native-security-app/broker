# Investigación: formato exacto de `definitions.json` (RabbitMQ)

Fecha: 2026-09-14
Fuentes consultadas (oficiales salvo que se indique lo contrario):

- https://www.rabbitmq.com/docs/definitions ("Schema Definition Export and Import")
- https://www.rabbitmq.com/docs/access-control
- https://www.rabbitmq.com/docs/dlx
- https://www.rabbitmq.com/docs/ttl
- https://www.rabbitmq.com/docs/passwords
- https://www.rabbitmq.com/docs/vhosts
- https://www.rabbitmq.com/docs/queues
- https://github.com/rabbitmq/cluster-operator/blob/main/docs/examples/import-definitions/definitions.json (ejemplo real, mantenido por el equipo de RabbitMQ/VMware/Broadcom)
- https://github.com/docker-library/rabbitmq (issues #107, #381, #428, #430) — imagen Docker oficial

**Nota metodológica**: varias páginas de rabbitmq.com se consultaron vía
WebFetch, que resume/extrae contenido en vez de servir el HTML crudo. Donde
el fetch no pudo confirmar algo textualmente, lo marco explícitamente como
"no confirmado" en vez de inventarlo.

---

## 0. Corrección importante sobre la premisa de la tarea

La tarea pedía investigar la variable de entorno `RABBITMQ_LOAD_DEFINITIONS`
en la imagen Docker oficial `rabbitmq:<tag>-management`. **Esa variable de
entorno NO existe en la imagen oficial `docker-library/rabbitmq`.**

- `RABBITMQ_LOAD_DEFINITIONS` sí existe, pero en la imagen de **Bitnami**
  (`bitnami/rabbitmq`), no en la oficial. No la uses si partes de
  `rabbitmq:4-management` (Docker Hub oficial / docker-library).
- Además, "de RabbitMQ 3.9 en adelante, todas las variables de entorno
  específicas de Docker [las `RABBITMQ_*` legacy] están deprecadas y ya no se
  usan" — según la propia página de Docker Hub del `rabbitmq` oficial. Hay
  que usar `rabbitmq.conf`, no variables `RABBITMQ_*`.
- El mecanismo correcto y confirmado (issues #381, #428, #430 de
  `docker-library/rabbitmq`, y coincide con `rabbitmq.com/docs/definitions`)
  es:
  1. Montar el archivo `definitions.json` dentro del contenedor, p. ej. en
     `/etc/rabbitmq/definitions.json`.
  2. En `rabbitmq.conf` (también montado), declarar:
     ```
     load_definitions = /etc/rabbitmq/definitions.json
     ```
  3. **Ojo con el nombre de la clave**: la clave antigua
     `management.load_definitions` existe y en algunas versiones funciona,
     pero el hilo docker-library#428 documenta que en el *primer* arranque
     del contenedor `management.load_definitions` falla en cargar policies de
     forma fiable, mientras que la clave núcleo `load_definitions` (sin el
     prefijo `management.`, soportada desde RabbitMQ 3.8.2+ según PR #430)
     funciona correctamente en el primer boot. **Recomendación: usar
     `load_definitions` (sin prefijo), no `management.load_definitions`.**
  4. `rabbitmq.com/docs/definitions` también documenta claves equivalentes
     para pre-configurar en el arranque sin pasar por el plugin de gestión:
     `definitions.local.path = /path/to/definitions/defs.json` (carga desde
     un archivo local) y `definitions.https.url = https://...` (carga desde
     una URL HTTPS). La página lo describe como "la forma recomendada de
     pre-configurar nodos en tiempo de despliegue".
  5. Import también es posible en caliente vía `rabbitmqctl import_definitions
     <file>` o vía la API HTTP del management plugin: `POST /api/definitions`.

Para docker-compose con la imagen oficial `rabbitmq:4.x-management`, el
patrón correcto es:

```yaml
services:
  rabbitmq:
    image: rabbitmq:4-management
    volumes:
      - ./rabbitmq/rabbitmq.conf:/etc/rabbitmq/rabbitmq.conf:ro
      - ./rabbitmq/definitions.json:/etc/rabbitmq/definitions.json:ro
```

con `rabbitmq.conf` conteniendo al menos:

```
load_definitions = /etc/rabbitmq/definitions.json
```

(además de la config de TLS/listeners que corresponda).

---

## 1. Estructura raíz de `definitions.json`

Confirmado por `rabbitmq.com/docs/definitions` (lista de secciones) y por el
ejemplo real de `cluster-operator` (que muestra todos los campos raíz
efectivamente presentes en un export real generado por RabbitMQ 3.8.11):

```json
{
  "rabbit_version": "3.8.11",
  "rabbitmq_version": "3.8.11",
  "product_name": "RabbitMQ",
  "product_version": "3.8.11",
  "users": [ ... ],
  "vhosts": [ ... ],
  "permissions": [ ... ],
  "topic_permissions": [ ... ],
  "parameters": [ ... ],
  "global_parameters": [ ... ],
  "policies": [ ... ],
  "queues": [ ... ],
  "exchanges": [ ... ],
  "bindings": [ ... ]
}
```

Notas:
- `rabbit_version` y `rabbitmq_version` aparecen ambos en el export real (el
  segundo parece ser el nombre más nuevo; ambos coexisten en 3.8.x). No pude
  confirmar en qué versión exacta se introdujo el segundo campo ni si
  `rabbit_version` se eliminó en 4.x — **no confirmado, tratar ambos como
  posibles/opcionales al escribir el archivo a mano**.
- Todas las claves de array (`users`, `vhosts`, `permissions`,
  `topic_permissions`, `parameters`, `global_parameters`, `policies`,
  `queues`, `exchanges`, `bindings`) son arrays de objetos JSON; si una
  sección no tiene elementos, RabbitMQ exporta un array vacío `[]` (visto en
  el ejemplo real: `"topic_permissions": []`, `"parameters": []`,
  `"policies": []`).
- Para importar a mano (no exportado desde un broker vivo) basta con incluir
  las secciones que se usan (`vhosts`, `users`, `permissions`, `exchanges`,
  `queues`, `bindings`); `rabbit_version`, `parameters`, `policies`,
  `global_parameters`, `topic_permissions` pueden omitirse o dejarse como
  array vacío — esto no lo dice explícitamente la doc, pero se infiere de
  que el import es tolerante a secciones ausentes (documentado de forma
  indirecta: la doc de definitions habla de "definitions" como conjunto de
  objetos, sin marcar ninguna sección como obligatoria salvo que se use).
  **Marco esto como inferencia razonable, no cita textual.**

---

## 2. Declarar un vhost dedicado (no `/`)

Confirmado por `rabbitmq.com/docs/vhosts`, ejemplo real citado ahí:

```json
{
  "name": "protected",
  "description": "",
  "metadata": {
    "description": "This virtual host is protected from deletion with a special metadata key",
    "tags": [],
    "default_queue_type": "classic",
    "protected_from_deletion": true
  },
  "tags": [],
  "default_queue_type": "classic"
}
```

Para un vhost simple dedicado al broker de este proyecto (sin protección
especial), basta con:

```json
{ "name": "security-scan", "description": "", "tags": [] }
```

Campos confirmados:
- `name` (string, obligatorio): nombre del vhost.
- `description` (string, opcional).
- `tags` (array de strings, opcional): etiquetas libres de vhost.
- `default_queue_type` (string, opcional): valores soportados
  `"quorum"`, `"stream"`, `"classic"` — determina el tipo de cola que se usa
  cuando una declaración de cola no especifica `x-queue-type`.
- `metadata` (objeto, opcional): contenedor con `description`, `tags`,
  `default_queue_type`, `protected_from_deletion` — parece ser la forma
  "nueva"/anidada que coexiste con los campos planos equivalentes en el
  nivel superior del objeto vhost (se ve en el ejemplo que ambos aparecen
  duplicados). No confirmé en qué versión se introdujo `metadata`.

El export real de `cluster-operator` (3.8.11, más antiguo) muestra vhosts
mucho más simples, sin `metadata` ni `default_queue_type`:

```json
{ "name": "hello-world" },
{ "name": "/" }
```

Esto confirma que los campos opcionales pueden omitirse por completo.

---

## 3. Exchange de tipo `topic`

`rabbitmq.com/docs/definitions` no mostró (vía WebFetch) un ejemplo
específico de exchange `topic`, pero el **schema de campos del objeto
exchange** está confirmado por el ejemplo real de `cluster-operator`
(ahí es `fanout`, pero la estructura de campos es la misma para `topic`):

```json
{
  "name": "example",
  "vhost": "hello-world",
  "type": "fanout",
  "durable": true,
  "auto_delete": false,
  "internal": false,
  "arguments": {}
}
```

Para el caso de este proyecto (exchange topic para `scan.request` /
`scan.outcome`), aplicando el mismo schema con `type: "topic"`:

```json
{
  "name": "security.scan.topic",
  "vhost": "security-scan",
  "type": "topic",
  "durable": true,
  "auto_delete": false,
  "internal": false,
  "arguments": {}
}
```

Campos confirmados (nombre y tipo):
- `name` (string): nombre del exchange.
- `vhost` (string): vhost al que pertenece.
- `type` (string): `"direct"`, `"fanout"`, `"topic"`, `"headers"` (los 4
  tipos estándar de AMQP 0-9-1; los valores exactos `direct/fanout/topic/
  headers` corresponden al protocolo, no son una cita textual de esta página
  puntual pero son el vocabulario estándar de RabbitMQ).
- `durable` (bool): sobrevive a reinicio del broker.
- `auto_delete` (bool): se borra cuando la última cola se desbindea.
- `internal` (bool): si es `true`, los publishers no pueden publicar
  directamente (solo se alimenta vía exchange-to-exchange bindings).
- `arguments` (objeto): x-arguments adicionales, p. ej.
  `"alternate-exchange"` para un AE. Puede ser `{}`.

---

## 4. Cola clásica con dead-lettering

### 4.1 Tipo de cola por defecto en RabbitMQ 4.x

Confirmado por búsqueda dirigida (varios resultados coincidentes, incl.
CloudAMQP y discusión oficial en `rabbitmq/rabbitmq-server`):

> "El equipo de RabbitMQ no planea cambiar el tipo de cola por defecto a
> quorum. El default sigue siendo **cola clásica** en RabbitMQ 4.0."

Lo que sí cambió en RabbitMQ 4.0: se **eliminó el mirroring de colas
clásicas** (deprecado desde hace 3 años, quitado en 4.0). Las colas clásicas
siguen soportadas como tipo no replicado; para alta disponibilidad el equipo
recomienda quorum queues, pero **no es obligatorio ni es el default** — se
puede seguir declarando colas clásicas explícitamente (sin
`x-queue-type` o con `"x-queue-type": "classic"`).

Conclusión para este proyecto: es seguro declarar colas clásicas de forma
explícita con `"arguments": {"x-queue-type": "classic"}` — esto es además
más explícito/a prueba de futuros cambios de default que omitir el
argumento.

### 4.2 Cómo se expresa el tipo de cola en `definitions.json`

**Importante**: el tipo de cola (`classic` / `quorum` / `stream`) **NO es un
campo de nivel superior `"type"` en el objeto de la cola**. Se expresa
exclusivamente como el x-argument `x-queue-type` dentro de `arguments`. Esto
está confirmado por el ejemplo real de `cluster-operator`:

```json
{
  "name": "qq1",
  "vhost": "hello-world",
  "durable": true,
  "auto_delete": false,
  "arguments": { "x-queue-type": "quorum" }
},
{
  "name": "cq1",
  "vhost": "hello-world",
  "durable": true,
  "auto_delete": false,
  "arguments": { "x-queue-type": "classic" }
}
```

Confirmé además, buscando explícitamente en `rabbitmq.com/docs/definitions`,
que la página **no menciona ningún campo `"type"` de nivel superior** en el
objeto cola — solo se referencia `x-queue-type` como argumento. Y
`rabbitmq.com/docs/queues` confirma: "queue type (...) se declara mediante
argumentos opcionales de cola (x-arguments)" y que `x-queue-type` "debe
fijarse en el momento de declarar la cola y no puede cambiarse después".

### 4.3 Dead-lettering: argumentos exactos

Confirmado por `rabbitmq.com/docs/dlx`:

- `x-dead-letter-exchange` (string): nombre del exchange que recibirá los
  mensajes muertos.
- `x-dead-letter-routing-key` (string): routing key a usar al re-publicar el
  mensaje muerto hacia `x-dead-letter-exchange`. Si se omite, se usa la
  routing key original del mensaje.
- La doc explícitamente **desaconseja** hardcodear estos x-arguments en el
  código del cliente ("no pueden actualizarse sin redesplegar
  aplicaciones") — lo correcto es declararlos en la topología
  (`definitions.json`), que es exactamente el enfoque de este repo.

Ejemplo combinado — cola clásica de trabajo con DLX, siguiendo el mismo
schema de campos confirmado en el ejemplo real (`name`, `vhost`, `durable`,
`auto_delete`, `arguments`):

```json
{
  "name": "ms-nmap.scan-request",
  "vhost": "security-scan",
  "durable": true,
  "auto_delete": false,
  "arguments": {
    "x-queue-type": "classic",
    "x-dead-letter-exchange": "security.scan.dlx",
    "x-dead-letter-routing-key": "scan.request.dead"
  }
}
```

---

## 5. Bindings con `routing_key` (exactas y wildcards)

Schema de campos confirmado por el ejemplo real de `cluster-operator`:

```json
{
  "source": "example",
  "vhost": "hello-world",
  "destination": "qq1",
  "destination_type": "queue",
  "routing_key": "",
  "arguments": {}
},
{
  "source": "example",
  "vhost": "hello-world",
  "destination": "cq1",
  "destination_type": "queue",
  "routing_key": "1234",
  "arguments": {}
}
```

Campos confirmados:
- `source` (string): nombre del exchange origen.
- `vhost` (string).
- `destination` (string): nombre de la cola o exchange destino.
- `destination_type` (string): `"queue"` o `"exchange"`.
- `routing_key` (string): para exchanges `topic`, admite wildcards AMQP
  estándar `*` (un segmento) y `#` (cero o más segmentos), separados por
  `.`. Esto es sintaxis estándar de AMQP 0-9-1 topic exchanges, no una cita
  puntual de la página de definitions, pero es coherente con el ejemplo
  usado en la tarea (`scan.outcome.#`, `scan.outcome.completed`).
- `arguments` (objeto): usado sobre todo por exchanges `headers`; para
  `topic`/`direct` normalmente `{}`.

Ejemplos aplicados al contrato de este proyecto:

```json
[
  {
    "source": "security.scan.topic",
    "vhost": "security-scan",
    "destination": "ms-nmap.scan-request",
    "destination_type": "queue",
    "routing_key": "scan.request",
    "arguments": {}
  },
  {
    "source": "security.scan.topic",
    "vhost": "security-scan",
    "destination": "ms-analisis.scan-outcome",
    "destination_type": "queue",
    "routing_key": "scan.outcome.#",
    "arguments": {}
  },
  {
    "source": "security.scan.topic",
    "vhost": "security-scan",
    "destination": "audit.scan-outcome-completed",
    "destination_type": "queue",
    "routing_key": "scan.outcome.completed",
    "arguments": {}
  }
]
```

(La cola `ms-nmap.scan-request` recibe todo lo publicado con routing key
`scan.request`; la cola de análisis recibe cualquier `scan.outcome.*` vía
`#`; y una tercera cola de ejemplo se bindea con la routing key exacta
`scan.outcome.completed` — ambos patrones son válidos y pueden coexistir en
el mismo exchange topic.)

---

## 6. Formato de `permissions`

Confirmado por `rabbitmq.com/docs/access-control` y por el ejemplo real:

```json
{
  "user": "guest",
  "vhost": "hello-world",
  "configure": ".*",
  "write": ".*",
  "read": ".*"
}
```

Campos confirmados:
- `user` (string): nombre de usuario RabbitMQ.
- `vhost` (string): vhost al que aplica el permiso (los permisos son
  siempre por-vhost).
- `configure` (string, regex): permiso para "crear o destruir recursos, o
  alterar su comportamiento" (declarar/borrar exchanges, colas, bindings).
- `write` (string, regex): permiso para "inyectar mensajes en un recurso"
  (publicar hacia un exchange).
- `read` (string, regex): permiso para "recuperar mensajes de un recurso"
  (consumir de una cola, o hacer binding leyendo desde un exchange).

Cita textual de la doc: "Los permisos se expresan como una tripleta de
expresiones regulares — una para configure, otra para write y otra para
read — sobre una base por-vhost."

### Patrones regex confirmados por la doc

- `'^$'` o `''` — no matchea nada salvo el string vacío ⇒ deniega
  completamente esa operación.
- `'^(amq\.gen.*|amq\.default)$'` — permite solo nombres autogenerados por
  el servidor (`amq.gen-*`) y el exchange por defecto (`amq.default`).
- `'.*'` — matchea todo (acceso completo a ese vhost para esa operación).
- `'^user2'` — matchea cualquier nombre que empiece con "user2".

También soporta variables de expansión en el backend interno de
autorización: `{username}`, `{vhost}`, `{client_id}` dentro del patrón.

### Ejemplo aplicado: usuario de mínimo privilegio

Caso pedido: un usuario que solo puede hacer `write` a un exchange
específico y `read` de una cola específica, pero nada más (ni declarar
recursos, ni tocar otros exchanges/colas).

Para un productor `ms-nmap` que solo publica hacia
`security.scan.topic` y no necesita declarar ni leer nada:

```json
{
  "user": "ms-nmap-producer",
  "vhost": "security-scan",
  "configure": "^$",
  "write": "^security\\.scan\\.topic$",
  "read": "^$"
}
```

Para un consumidor `ms-analisis` que solo lee de su cola dedicada
`ms-analisis.scan-outcome` (y no publica ni declara nada):

```json
{
  "user": "ms-analisis-consumer",
  "vhost": "security-scan",
  "configure": "^$",
  "write": "^$",
  "read": "^ms-analisis\\.scan-outcome$"
}
```

Nota importante (inferencia a partir de la semántica documentada de
`configure`/`write`/`read`, no cita textual): si un servicio necesita
**consumir** de una cola que él mismo declara al arrancar (patrón típico con
`queue_declare` idempotente en el cliente), también necesita `configure`
sobre el nombre exacto de esa cola, no solo `read` — porque declarar/verificar
una cola es una operación de `configure`. Si la topología ya la declara este
repo vía `definitions.json` y el servicio consumidor solo hace
`basic.consume` (nunca `queue.declare`), con `read` (y `configure: "^$"`)
basta.

### CLI equivalente (para tests/documentación cruzada)

```
rabbitmqctl set_permissions -p "security-scan" "ms-nmap-producer" "^$" "^security\.scan\.topic$" "^$"
```

---

## 7. Formato de `users`

Confirmado por `rabbitmq.com/docs/definitions` (ejemplo textual citado en la
propia doc) y por el ejemplo real de `cluster-operator`:

```json
{
  "name": "guest",
  "password_hash": "9/1i+jKFRpbTRV1PtRnzFFYibT3cEpP92JeZ8YKGtflf4e/u",
  "tags": ["administrator"]
}
```

Versión más completa vista en el export real (3.8.11):

```json
{
  "name": "hello-world",
  "password_hash": "JQ6+ZVMAIIpmGS/pXb9Q6elneY94TrchYGYJAKE9wtRiIpRt",
  "hashing_algorithm": "rabbit_password_hashing_sha256",
  "tags": "administrator",
  "limits": {}
}
```

Campos confirmados:
- `name` (string).
- `password_hash` (string, base64): hash de la contraseña, **no** la
  contraseña en texto plano.
- `hashing_algorithm` (string, opcional): identifica el algoritmo usado,
  p. ej. `"rabbit_password_hashing_sha256"`. Visto en el export real; no until
  este campo aparece en el ejemplo minimalista de la doc, así que
  parece opcional/informativo — RabbitMQ usa SHA-256 por defecto si se omite.
- `tags` (string o array de strings — el ejemplo de la doc usa array
  `["administrator"]`, el export real usa string `"administrator"`; ambas
  formas aparecen en fuentes oficiales/reales, así que **ambas son
  aceptadas**, aunque no encontré una nota explícita de la doc confirmando
  la equivalencia — lo marco como observación empírica de dos fuentes
  reales, no como cita textual única).
- `limits` (objeto, opcional): límites por-usuario (p. ej. `max-connections`,
  `max-channels`); `{}` si no hay límites.

### Cálculo del `password_hash` (confirmado por `rabbitmq.com/docs/passwords`)

Algoritmo por defecto: **SHA-256**. Proceso documentado, en 5 pasos:
1. Generar una sal aleatoria de 32 bits.
2. Anteponer la sal a la representación UTF-8 de la contraseña deseada.
3. Aplicar SHA-256 sobre ese valor concatenado.
4. Anteponer la sal de nuevo al resultado del hash.
5. Codificar el valor combinado en base64 → ese string final es
   `password_hash`.

Ejemplo citado en la doc: para la contraseña `test12` con sal `908D C60A`,
el resultado final es `"kI3GCqW5JLMJa4iX1lo7X4D6XbYqlLgxIs30+P6tENUV2POR"`.

### ¿Se soporta `"password"` en texto plano en `definitions.json`?

**No pude confirmarlo con una fuente textual explícita.** La doc de
`definitions` solo dice que "los datos de usuario exportados contienen
hashes de contraseña, así como información de la función de hashing usada"
— es decir, lo que **RabbitMQ exporta** siempre es `password_hash`, nunca
texto plano. No encontré una afirmación explícita de que un campo
`"password"` en texto plano sea (o no sea) aceptado al **importar**. Dado
que no puedo confirmarlo, **no asuman que funciona**: la práctica correcta y
documentada es siempre precalcular `password_hash` (con el algoritmo de
arriba) y usar ese campo — nunca comitear contraseñas en texto plano en un
archivo versionado como `definitions.json`, algo doblemente crítico en este
repo por el requisito de `docs/security-scope.md` de no exponer secretos.
Recomendación operativa: generar los usuarios en tiempo de despliegue (script
que calcula `password_hash` a partir de un secreto inyectado), no hardcodear
en el `definitions.json` versionado en git.

---

## 8. Patrón TTL + DLX para reintentos acotados, y formato de `x-death`

### 8.1 ¿RabbitMQ soporta este patrón sin plugin de delayed-message?

Confirmado (`rabbitmq.com/docs/ttl` + `rabbitmq.com/docs/dlx`): sí. El
mecanismo estándar es:

1. Declarar una cola de **retry** con `x-message-ttl` corto (p. ej. 5000 =
   5s) y `x-dead-letter-exchange` apuntando de vuelta al exchange/routing
   key que reencola hacia la cola original de trabajo.
2. Cuando un consumidor falla y hace `basic.nack`/`basic.reject` con
   `requeue=false` sobre la cola de trabajo, el mensaje se dead-letra hacia
   un exchange "retry" que lo enruta a la cola de retry.
3. En la cola de retry el mensaje espera el TTL completo (no se consume
   activamente); al expirar, RabbitMQ lo dead-letra automáticamente (porque
   la cola de retry tiene su propio `x-dead-letter-exchange` configurado)
   de vuelta hacia el exchange/cola de trabajo original.
4. Esto evita necesitar el plugin `rabbitmq-delayed-message-exchange` para
   reintentos acotados de duración fija.

Cita relevante de la doc de TTL: "el valor del argumento o política TTL debe
ser un entero no negativo... que describe el periodo TTL en milisegundos" —
y la doc de DLX confirma que un mensaje puede dead-letrarse por tres motivos
(`rejected`/`expired`/`maxlen`, más `delivery_limit` para quorum queues),
donde `expired` es exactamente el caso de expiración por TTL.

Ejemplo de declaración de la cola de retry (siguiendo el mismo schema
confirmado en la sección 4):

```json
{
  "name": "ms-nmap.scan-request.retry",
  "vhost": "security-scan",
  "durable": true,
  "auto_delete": false,
  "arguments": {
    "x-queue-type": "classic",
    "x-message-ttl": 5000,
    "x-dead-letter-exchange": "security.scan.topic",
    "x-dead-letter-routing-key": "scan.request"
  }
}
```

(Nadie consume directamente de `ms-nmap.scan-request.retry` — es solo una
"sala de espera"; al expirar el TTL, el mensaje se dead-letra
automáticamente de vuelta a `security.scan.topic` con routing key
`scan.request`, re-entregándolo a `ms-nmap.scan-request`.)

**Advertencia documentada que aplica a este patrón** (relevante para el
test con `lapin`/`testcontainers`): la doc de TTL indica que en **quorum
queues**, la expiración/dead-lettering de un mensaje solo se evalúa "cuando
el mensaje llega a la cabeza de la cola" (no de forma perezosa en cualquier
punto) — es decir, un mensaje viejo puede quedar "atascado" detrás de un
mensaje sin TTL vencido si hay reordenamiento en la cola. Con **colas
clásicas** (que es lo que usa este proyecto para la cola de retry, ver
sección 4.1) el comportamiento de expiración es más predecible. No confirmé
el detalle exacto de la semántica en colas clásicas comparado con quorum más
allá de esta mención — si el test de integración necesita verificar timing
exacto de expiración, conviene revisar `rabbitmq.com/docs/ttl` directamente
en ese momento en vez de asumir.

### 8.2 Formato del header `x-death`

Confirmado por `rabbitmq.com/docs/dlx`. Cuando un mensaje se dead-letra,
RabbitMQ añade (o actualiza) el header `x-death` en el mensaje —en AMQP
0-9-1; en AMQP 1.0 el equivalente es la anotación `x-opt-deaths`—. Es un
**array de objetos**, uno por cada combinación (cola, motivo) por la que el
mensaje ha muerto, con estos campos:

| Campo          | Tipo             | Significado                                                                 |
|----------------|------------------|-------------------------------------------------------------------------------|
| `queue`        | string           | Nombre de la cola desde la que el mensaje fue dead-letrado.                  |
| `reason`       | string           | Motivo: `rejected`, `expired`, `maxlen`, o `delivery_limit`.                  |
| `count`        | long             | Cuántas veces este mensaje fue dead-letrado desde esta cola por este motivo. |
| `time`         | timestamp        | Momento del evento de dead-lettering.                                        |
| `exchange`     | string           | Exchange al que el mensaje fue publicado antes de ser dead-letrado.          |
| `routing-keys` | array de strings | Routing keys del mensaje (incluye `CC`, excluye `BCC`).                      |

Además, RabbitMQ añade headers adicionales que resumen el primer y el
último evento de muerte: `x-first-death-queue`, `x-first-death-reason`,
`x-first-death-exchange`, `x-last-death-queue`, `x-last-death-reason`,
`x-last-death-exchange` (nombres inferidos del patrón `x-first-death-*` /
`x-last-death-*` mencionado por la doc; confirma la existencia de ambos
prefijos pero no cité el listado completo de sub-campos de cada uno).

Para un test de integración con `lapin` que quiera leer el conteo de
reintentos: el header `x-death` llega como parte de
`BasicProperties::headers()` (`FieldTable` de `lapin`/`amq-protocol`); hay
que:
1. Leer el `FieldTable` del header `x-death` del mensaje entregado a la cola
   original tras el ciclo de retry.
2. Iterarlo como array de tablas (`AMQPValue::FieldArray` de
   `AMQPValue::FieldTable`), buscar la entrada cuyo campo `queue` coincide
   con el nombre de la cola de trabajo original (o la de retry, según qué
   conteo se quiera) y leer su campo `count` (entero largo).

No verifiqué el mapeo exacto de tipos `lapin`/`amq-protocol` para
`x-death` con una prueba real — esto es una traducción directa del formato
AMQP 0-9-1 documentado arriba a los tipos que expone `lapin`, no una cita de
la doc de `lapin`. Si se implementa el test, conviene imprimir el
`FieldTable` crudo una vez para confirmar el mapeo de variantes de
`AMQPValue` antes de fijar el parsing.

---

## Resumen ejecutivo (para quien no lea todo el documento)

1. **No existe `RABBITMQ_LOAD_DEFINITIONS`** en la imagen oficial. Usar
   `load_definitions = /etc/rabbitmq/definitions.json` en `rabbitmq.conf`
   (no `management.load_definitions`, que falla en el primer boot según
   docker-library/rabbitmq#428).
2. Estructura raíz confirmada con ejemplo real: `rabbit_version`,
   `users`, `vhosts`, `permissions`, `topic_permissions`, `parameters`,
   `global_parameters`, `policies`, `queues`, `exchanges`, `bindings`.
3. El tipo de cola (classic/quorum/stream) va en
   `queues[].arguments["x-queue-type"]`, **no** en un campo `"type"` de
   nivel superior.
4. RabbitMQ 4.0 **no cambió el default** de tipo de cola: sigue siendo
   `classic`. Lo que se quitó en 4.0 fue el mirroring de colas clásicas.
5. Dead-lettering: `x-dead-letter-exchange` + `x-dead-letter-routing-key`
   en `arguments`, confirmado con ejemplos.
6. `permissions` son 3 regex (`configure`/`write`/`read`) por usuario+vhost;
   `"^$"` deniega, `"^nombre\\.exacto$"` restringe a un recurso.
7. `users[].password_hash` es SHA-256(salt+password)+salt en base64; no se
   confirmó soporte de contraseña en texto plano — no usarlo, usar hash
   siempre.
8. Patrón retry = cola con `x-message-ttl` corto + `x-dead-letter-exchange`
   apuntando de vuelta al exchange original; confirmado como el mecanismo
   estándar sin necesitar el plugin delayed-message. Header `x-death` es un
   array de objetos con `queue`, `reason`, `count`, `time`, `exchange`,
   `routing-keys`.
