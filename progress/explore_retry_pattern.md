# Investigación: patrón de reintentos acotados antes de dead-letter

> Investigación para la feature `topology_definition` (RNF-06). Fuentes:
> documentación oficial `rabbitmq.com/docs`, consultadas 2026-09-14.
> Este documento da una **recomendación única y cerrada**, no una lista de
> opciones sin decidir.

## TL;DR — decisión

**Usar `x-delivery-limit` en colas quorum (nativo de RabbitMQ, no el
patrón clásico de "cola de retry intermedia con TTL").** Cada cola
principal (`ms-nmap.scan-requests`, `ms-analisis.scan-outcomes`,
`gateway.scan-outcomes`, y luego `ms-nmap.scan-cancellations` en la feature
`cancellation_contract`) se declara como **cola quorum** con:

```json
"arguments": {
  "x-queue-type": "quorum",
  "x-delivery-limit": 3,
  "x-dead-letter-exchange": "<nombre-del-dlx-de-ese-exchange>",
  "x-dead-letter-routing-key": "<routing-key-exclusiva-de-esta-cola>"
}
```

- **N = 3** reintentos (4 entregas totales: 1 original + 3 redelivery).
- **Sin cola de retry intermedia, sin `x-message-ttl` de por medio.** El
  reintento es inmediato (RabbitMQ vuelve a entregar el mensaje dentro de la
  misma cola); RabbitMQ decide automáticamente cuándo agotó el máximo y
  dead-letterea con `reason: "delivery_limit"` — no requiere lógica de
  consumidor ni de test para contar `x-death[].count` y decidir.
- Justificación completa, comparación con el patrón clásico y sintaxis
  exacta para `rabbitmq/definitions.json` (incluyendo el caso de
  `scan.outcomes.dlx`, compartido por dos colas principales que necesitan
  cada una su propia `.dlq`) más abajo.

---

## 1. El patrón clásico: "cola de retry con TTL" (parking lot / delayed retry)

Documentado en <https://www.rabbitmq.com/docs/dlx> y
<https://www.rabbitmq.com/docs/ttl>. Mecánica:

1. **Cola principal** (`ms-nmap.scan-requests`, p. ej.) declara:
   - `x-dead-letter-exchange`: exchange de retry (no el exchange original).
   - `x-dead-letter-routing-key` (opcional): routing key hacia la cola de
     retry.
2. Cuando el consumidor hace `basic.nack`/`basic.reject` con
   `requeue=false` (o el mensaje expira, o se excede `x-max-length`), el
   mensaje se **dead-lettera** hacia ese exchange de retry.
3. **Cola de retry** (`ms-nmap.scan-requests.retry`, p. ej.) declara:
   - `x-message-ttl`: TTL corto (p. ej. `2000`, en milisegundos).
   - `x-dead-letter-exchange`: el exchange **original** (`scan.requests`) o
     directamente la cola original vía el exchange por defecto.
   - **Nadie consume esta cola** — solo existe para que el TTL expire.
4. Al expirar el TTL, el mensaje vuelve a "morir" (esta vez con
   `reason: "expired"`) y RabbitMQ lo vuelve a publicar en el exchange
   original → vuelve a la cola principal → nuevo intento de consumo.

Cada ciclo por (cola, razón) se registra/actualiza en el header `x-death`
(ver §2). El propio documento de DLX advierte además: *"hardcoded
`x-arguments` are strongly recommended against"* — RabbitMQ recomienda usar
**políticas** en vez de argumentos fijos en la declaración de cola quando
sea posible, precisamente para poder ajustar TTL/DLX sin re-declarar colas.

### Quién decide "ya se acabaron los reintentos"

**El propio consumidor**, no RabbitMQ. RabbitMQ solo sabe "dead-letterear
según TTL/rechazo"; **no existe** en el patrón clásico un mecanismo nativo
que cuente cuántas veces pasó por el ciclo y decida enviarlo a la `.dlq`
definitiva en vez de reencolarlo otra vez. Esa decisión la implementa el
código de aplicación:

```text
al recibir un mensaje:
  entries = header x-death (array)
  entry_de_esta_cola = entries.find(e => e.queue == "ms-nmap.scan-requests"
                                       && e.reason == "rejected")
  if entry_de_esta_cola.count >= N:
      nack(requeue=false)  # pero apuntando esta vez a la exchange de
                            # la .dlq DEFINITIVA, no a la de retry —
                            # normalmente requiere DOS DLX distintos
                            # (uno "de retry", otro "definitivo") y que
                            # el consumidor decida a cuál mandarlo, o
                            # publicar manualmente a la .dlq definitiva.
  else:
      nack(requeue=false)  # va al DLX de retry, vuelve a intentarse
```

Esto es exactamente lo que confirma la documentación oficial de dead
lettering: el header `x-death` es *información*, no *control* — RabbitMQ lo
adjunta pero no actúa sobre él. La lógica de "ya llegó a N, ahora sí a la
`.dlq` final" vive en el consumidor.

**Implicación para este repo**: este repo no tiene consumidores de
producción (`docs/architecture.md`: "cada servicio implementa su propio
cliente RabbitMQ... no se publica ninguna librería compartida"). El único
código que existe aquí es el crate de verificación
(`testcontainers`+`lapin`). Implementar el patrón clásico completo aquí
significaría o bien (a) escribir en el crate de verificación un
"pseudo-consumidor" que simula la lógica de conteo de `x-death` solo para
poder probar el ciclo — lógica de aplicación que no le corresponde a este
repo, que solo define infraestructura — o bien (b) dejar esa lógica sin
probar y confiar en que cada servicio real (`ms-nmap`, etc.) la implemente
correctamente por su cuenta, sin que este repo pueda verificarlo. Ninguna
de las dos es satisfactoria dado el alcance declarado del repo.

## 2. El header `x-death`: estructura exacta

Confirmado en <https://www.rabbitmq.com/docs/dlx>. Cada entrada del array
`x-death` (AMQP 0.9.1; en AMQP 1.0 es la anotación `x-opt-deaths`) tiene:

| Campo | Tipo | Significado |
|---|---|---|
| `queue` | longstr | Nombre de la cola desde la que se dead-letteró |
| `reason` | longstr | `rejected`, `expired`, `maxlen`, o **`delivery_limit`** |
| `count` | long | Cuántas veces se dead-letteró desde esa cola por esa razón |
| `time` | timestamp | Cuándo ocurrió la **primera** vez (cola, razón) |
| `exchange` | longstr | Exchange de publicación original |
| `routing-keys` | array de longstr | Routing keys originales (sin BCC) |
| `original-expiration` | longstr (opcional) | TTL original del mensaje |

Punto clave: el historial se **comprime por el par `{queue, reason}`** — no
se añade una entrada nueva cada vez, se **incrementa `count`** en la
entrada existente si el par `(queue, reason)` ya apareció. Por eso, en el
patrón clásico, `x-death[i].count` (para la entrada de la cola principal)
es exactamente el número de veces que el mensaje completó un ciclo
retry→vuelta — es el dato que el consumidor tendría que inspeccionar para
decidir el corte en N.

`delivery_limit` está documentado explícitamente como una de las cuatro
razones posibles de `reason`, y es la que usa el mecanismo nativo de colas
quorum descrito en §3 — es decir, **incluso sin construir el patrón
clásico, `x-death` seguirá reflejando el motivo `delivery_limit`** cuando
el mensaje llegue finalmente a la `.dlq`, lo cual sigue siendo útil para
diagnóstico/observabilidad aunque no se use para decidir el corte.

## 3. `x-delivery-limit` (colas quorum, RabbitMQ 3.10+): mecanismo nativo

Confirmado en <https://www.rabbitmq.com/docs/quorum-queues> (sección
"Poison Message Handling"). Este repo pinea `rabbitmq:4.3.5-management`
(`docker-compose.yml`), muy por encima del mínimo 3.10 — sin restricción de
compatibilidad.

- **Solo aplica a colas quorum** (`x-queue-type: quorum`), no a colas
  clásicas.
- **Argumento de cola**: `x-delivery-limit` (entero). **Clave de
  política**: `delivery-limit`.
- Cada mensaje lleva un contador interno `delivery-count` que RabbitMQ
  incrementa cada vez que se reintenta la entrega (nack/reject con
  requeue, cierre de canal/sesión con mensajes sin confirmar). Desde
  RabbitMQ 4.3, se basa en `delivery-count` (no en `acquired-count`) — esto
  significa que un `nack`/`modify` explícito **sin** intención de fallo
  permanente puede excluirse del conteo si la librería cliente lo señala
  así; para nuestro caso (fallos reales de procesamiento) el conteo se
  comporta como se espera: cada intento fallido cuenta.
- **Cuando el mensaje "es devuelto más veces que el límite"** (cita
  literal de la documentación), RabbitMQ lo **descarta** o, si hay
  `x-dead-letter-exchange` configurado, lo **dead-lettera
  automáticamente** con `reason: "delivery_limit"` — **sin intervención
  del consumidor ni de ningún otro componente**.
- Default desde RabbitMQ 4.0: `delivery-limit = 20` si no se especifica.
  Este repo lo **fija explícitamente a un valor bajo** (ver §5) porque el
  requisito exige que N esté "documentado y justificado", no implícito en
  un default de la imagen.
- Deshabilitarlo (`x-delivery-limit: -1`) restaura el comportamiento sin
  límite de RabbitMQ ≤3.13 — explícitamente no recomendado por la propia
  documentación.

### Por qué esto resuelve el requisito sin cola de retry intermedia

No hace falta ninguna cola/exchange adicional de "retry": la cola
principal, por sí sola, reintenta la entrega internamente hasta
`x-delivery-limit` veces y **RabbitMQ mismo** decide el corte hacia la
`.dlq` — exactamente la decisión que en el patrón clásico recaía en el
consumidor. Esto es lo que el propio pedido de investigación (punto 2)
pregunta si existe: sí existe, y es justo esto.

## 4. Comparación para este caso concreto

| | Cola de retry + TTL (clásico) | `x-delivery-limit` (quorum, nativo) |
|---|---|---|
| Colas/exchanges extra por cola principal | +1 exchange de retry, +1 cola de retry (sin consumidor) | Ninguno |
| Quién decide "ya se acabaron los N reintentos" | El consumidor de aplicación, inspeccionando `x-death[].count` | RabbitMQ, automáticamente |
| Verificable solo con `definitions.json` + testcontainers/lapin (sin consumidor real) | **No** — requiere simular lógica de consumidor en el crate de verificación, que es lógica de aplicación fuera del alcance de este repo | **Sí** — un test solo necesita `nack(requeue=true)` N veces y comprobar que el mensaje no cae en la `.dlq` hasta la N+1-ésima vez; el corte lo hace el broker |
| Retraso entre reintentos | Sí, el TTL de la cola de retry (p. ej. 2000ms) — cada ciclo completo tarda ese TTL | No hay retraso nativo entre reintentos; la redelivery es inmediata |
| Riesgo de test lento/frágil por esperar TTLs | Sí — el test de integración debe esperar N × TTL en tiempo real para completar el ciclo | No — el test corre en milisegundos, sin `sleep` |
| Tipo de cola requerido | Cualquiera (clásica o quorum) | Solo colas **quorum** |
| Encaja con "no consumidores de producción en este repo" (`docs/architecture.md`) | No — obliga a este repo a codificar lógica de decisión que "no le corresponde" | Sí — la decisión de corte es responsabilidad de RabbitMQ, no de ningún consumidor (real o simulado) |
| Recomendado oficialmente para mensajes "envenenados" (poison message) | No es el mecanismo que RabbitMQ documenta para este propósito específico | Sí — la propia sección se llama "Poison Message Handling" |

**Conclusión de la comparación**: para este repo, donde el requisito
explícito es "verificable solo con `definitions.json`... sin necesitar un
consumidor real" y donde el propio `docs/architecture.md` prohíbe construir
lógica de aplicación/cliente aquí, `x-delivery-limit` no es solo la opción
más simple — es la **única** de las dos que se puede verificar de forma
honesta sin violar el alcance del repo.

## 5. Valores recomendados de laboratorio

- **N = 3** (`"x-delivery-limit": 3`).
  - Justificación: 3 reintentos (4 entregas totales) es el valor por
    defecto de facto en la mayoría de frameworks de mensajería con
    reintentos acotados (p. ej. patrones "3 strikes"), suficiente para
    absorber fallos transitorios cortos (blip de red, deadlock momentáneo)
    sin enmascarar un fallo persistente durante mucho tiempo. Es **mucho**
    menor que el default de RabbitMQ 4.0+ (20), que dejaría un mensaje
    "envenenado" reintentando silenciosamente 20 veces antes de ser
    visible en la `.dlq` — inaceptable para un requisito que exige "N...
    razonable, no arbitrario" y observable rápido (RNF-09/RNF-10,
    observabilidad).
  - Para un test de integración: con `x-delivery-limit=3`, el test
    (`testcontainers`+`lapin`) publica un mensaje, lo consume y hace
    `nack(requeue=true)` **3 veces seguidas** (simulando 3 fallos de
    procesamiento) y comprueba que en ninguna de esas 3 veces el mensaje
    apareció en la `.dlq`; al cuarto intento de entrega, en vez de
    reentregarlo, RabbitMQ lo dead-letterea automáticamente a la `.dlq` —
    el test lo confirma consumiendo de la `.dlq` con `reason:
    "delivery_limit"` presente en `x-death`. Todo esto ocurre en
    milisegundos, sin `sleep`, por lo que cae ampliamente dentro de
    cualquier timeout razonable de test (no aplica el problema de
    "esperar el ciclo completo en <10s" que sí tendría el patrón clásico
    con TTL, porque aquí no hay TTL que esperar).

- **TTL**: **no aplica como mecanismo del broker** con esta decisión —
  `x-delivery-limit` no introduce demora entre reintentos, así que no hay
  un `x-message-ttl` de cola de retry que fijar. Esto es una desviación
  deliberada de la redacción "p. ej." de `feature_list.json` (que describe
  el patrón clásico solo como *ejemplo* de "N reintentos acotados", no como
  mandato); el requisito duro que sí se cumple literalmente es "NO un
  TTL/rechazo único que vaya directo a la `.dlq`" — con N=3 hay tres
  reintentos reales antes de la `.dlq`, no uno.
  - Si en el futuro un consumidor real (p. ej. `ms-nmap`) quisiera espaciar
    sus reintentos para no reintentar en caliente contra un objetivo que
    acaba de fallar, eso es una decisión de **backoff en el cliente**
    (p. ej. dormir 500ms–2000ms antes de cada `nack`), no de la topología
    del Broker — coherente con "cada servicio implementa su propio cliente
    RabbitMQ" (`docs/architecture.md`). Se documenta aquí como
    recomendación para quien implemente ese cliente, no como argumento de
    cola: **backoff sugerido 500ms–2000ms entre reintentos**, sin que este
    repo lo imponga ni lo pueda verificar (no hay consumidor real aquí).

## 6. Sintaxis exacta para `rabbitmq/definitions.json`

Caso simple — `ms-nmap.scan-requests` (única cola sobre `scan.requests`,
por lo que su DLX puede ser 1:1 con `ms-nmap.scan-requests.dlq`):

```json
{
  "name": "ms-nmap.scan-requests",
  "vhost": "security-app",
  "durable": true,
  "auto_delete": false,
  "arguments": {
    "x-queue-type": "quorum",
    "x-delivery-limit": 3,
    "x-dead-letter-exchange": "scan.requests.dlx",
    "x-dead-letter-routing-key": "ms-nmap.scan-requests.dead"
  }
}
```

```json
{
  "name": "ms-nmap.scan-requests.dlq",
  "vhost": "security-app",
  "durable": true,
  "auto_delete": false,
  "arguments": {}
}
```

```json
{
  "source": "scan.requests.dlx",
  "vhost": "security-app",
  "destination": "ms-nmap.scan-requests.dlq",
  "destination_type": "queue",
  "routing_key": "ms-nmap.scan-requests.dead",
  "arguments": {}
}
```

(`scan.requests.dlx` se declara como exchange tipo `direct` — no hace falta
`topic`, porque aquí solo enruta hacia un único destino final.)

Caso con **una sola DLX compartida por dos colas principales** —
`scan.outcomes.dlx` recibe los dead-letters tanto de
`ms-analisis.scan-outcomes` como de `gateway.scan-outcomes`, y cada una
necesita terminar en **su propia** `.dlq` (requisito explícito de
`feature_list.json`). Se resuelve con `x-dead-letter-routing-key` distinto
por cola, y `scan.outcomes.dlx` como exchange `direct` con un binding por
cada `.dlq`:

```json
{
  "name": "ms-analisis.scan-outcomes",
  "arguments": {
    "x-queue-type": "quorum",
    "x-delivery-limit": 3,
    "x-dead-letter-exchange": "scan.outcomes.dlx",
    "x-dead-letter-routing-key": "ms-analisis.scan-outcomes.dead"
  }
},
{
  "name": "gateway.scan-outcomes",
  "arguments": {
    "x-queue-type": "quorum",
    "x-delivery-limit": 3,
    "x-dead-letter-exchange": "scan.outcomes.dlx",
    "x-dead-letter-routing-key": "gateway.scan-outcomes.dead"
  }
}
```

con dos bindings en `scan.outcomes.dlx` (uno por routing key, cada uno
hacia su propia `.dlq`): `ms-analisis.scan-outcomes.dead →
ms-analisis.scan-outcomes.dlq` y `gateway.scan-outcomes.dead →
gateway.scan-outcomes.dlq`.

**Nota sobre argumentos fijos vs. políticas**: la documentación oficial de
RabbitMQ recomienda en general usar políticas en vez de argumentos fijos
(`x-arguments` hardcodeados) para poder ajustar TTL/DLX sin re-declarar
colas. Para este repo se recomienda **no** seguir esa recomendación
genérica y usar argumentos explícitos por cola como en los ejemplos de
arriba: `docs/architecture.md` ya fija como decisión de diseño que "la
topología se declara de forma imperativa en `rabbitmq/definitions.json`...
no se crea a mano ni por scripts imperativos sueltos" — con solo 3-4 colas
principales, cada una con su propia `.dlq` (no un patrón compartido por
muchas colas homogéneas, que es el caso que justifica usar políticas),
declarar el límite y el DLX directamente en cada cola es igual de auditable
para un `reviewer` y evita la indirección de una política aparte que habría
que mantener sincronizada con los mismos nombres.

## 7. Cómo lo verifica el test de integración (sin consumidor real)

Con `testcontainers`+`lapin`, sin necesitar ningún cliente de producción:

1. Declarar/confirmar la topología de arriba ya cargada vía
   `RABBITMQ_LOAD_DEFINITIONS`.
2. Publicar un mensaje de prueba en, p. ej., `scan.requests` con la
   routing key correcta.
3. Consumir de `ms-nmap.scan-requests` con `basic.consume`
   (`no_ack: false`), y por cada entrega hacer `basic.nack(requeue=true)`
   — repetir esto **3 veces** (= N), confirmando en cada una que la `.dlq`
   sigue vacía (`basic.get` sobre `ms-nmap.scan-requests.dlq` devuelve
   `None`).
4. Consumir una **cuarta** vez: en vez de recibir el mensaje otra vez en
   `ms-nmap.scan-requests`, debe aparecer en `ms-nmap.scan-requests.dlq`.
   Verificar que el header `x-death` de ese mensaje contiene una entrada
   con `reason: "delivery_limit"` y `queue: "ms-nmap.scan-requests"` — esto
   prueba que **pasó por el ciclo de reintentos** (no cayó directo a la
   `.dlq` en el primer fallo), cumpliendo literalmente el criterio de
   aceptación de `feature_list.json`: *"se verifica que efectivamente pasó
   por el ciclo de reintentos (no cayó directo a la primera)"*.

Todo esto sin TTLs que esperar ni lógica de consumidor que simular — el
test solo publica, nackea N veces y comprueba el resultado final.

## Fuentes

- [Dead Letter Exchanges — RabbitMQ Docs](https://www.rabbitmq.com/docs/dlx) — sintaxis de `x-dead-letter-exchange`/`x-dead-letter-routing-key`, estructura exacta del header `x-death` (campos `queue`, `reason`, `count`, `time`, `exchange`, `routing-keys`, `original-expiration`), compresión por par `{queue, reason}`, las cuatro razones posibles (`rejected`, `expired`, `maxlen`, `delivery_limit`), y la recomendación de usar políticas en vez de `x-arguments` hardcodeados.
- [Time-To-Live and Expiration — RabbitMQ Docs](https://www.rabbitmq.com/docs/ttl) — sintaxis de `x-message-ttl` (TTL por cola) y `x-expires`; confirma que un mensaje expirado se dead-lettera al llegar a la cabeza de la cola (colas quorum y clásicas).
- [Quorum Queues — RabbitMQ Docs](https://www.rabbitmq.com/docs/quorum-queues) (sección "Poison Message Handling") — `x-delivery-limit`/`delivery-limit`, default de 20 desde RabbitMQ 4.0, comportamiento basado en `delivery-count` desde RabbitMQ 4.3, dead-letter automático con `reason: "delivery_limit"` cuando se excede el límite, y que `x-delivery-limit=-1` deshabilita el límite (no recomendado).
- [At-Least-Once Dead Lettering — RabbitMQ Blog](https://www.rabbitmq.com/blog/2022/03/29/at-least-once-dead-lettering) — contexto adicional sobre `delivery_limit` como razón de dead-lettering ("mensaje reencolado demasiadas veces").

## Contexto del repo consultado para esta recomendación

- `feature_list.json` (feature `topology_definition`, id 2) — texto exacto
  del criterio de aceptación sobre reintentos acotados.
- `docs/architecture.md` — decisiones ya tomadas: topología declarativa en
  `definitions.json`, sin librería/cliente compartido, sin consumidores de
  producción en este repo, verificación solo vía `testcontainers`+`lapin`.
- `docs/security-scope.md` — mínimo privilegio y retención de mensajes
  (relevante para no sobre-diseñar TTLs/retención en las `.dlq`, fuera del
  alcance de esta investigación pero mencionado para no contradecirlo).
- `docker-compose.yml` — imagen pineada `rabbitmq:4.3.5-management`,
  confirma compatibilidad total con `x-delivery-limit` (requiere 3.10+).
