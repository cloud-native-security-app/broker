# syntax=docker/dockerfile:1
#
# Imagen custom de RabbitMQ para despliegue en AWS Fargate.
#
# En docker-compose.yml estos 3 archivos se montan por bind-mount desde el
# host (ver ../../docker-compose.yml, servicio `rabbitmq`), pero Fargate no
# soporta bind-mounts de archivos del host — se hornean aquí en su lugar, en
# las mismas rutas que antes proveía el bind-mount, así que rabbitmq.conf no
# necesita ningún cambio.
FROM rabbitmq:4.3.5-management

COPY definitions.json /etc/rabbitmq/definitions.json
COPY rabbitmq.conf /etc/rabbitmq/rabbitmq.conf
COPY tls/ /etc/rabbitmq/tls/

# Fargate resetea a root el ownership de /var/lib/rabbitmq (VOLUME de la
# imagen base) en cada arranque, y un `chown` hecho en el entrypoint en
# tiempo de ARRANQUE no alcanza a dejarlo utilizable por el usuario
# `rabbitmq` en el almacenamiento efímero real de Fargate (probado: sigue
# fallando incluso con ese chown). En vez de pelear con los permisos de ese
# path en runtime, se saca de ahí por completo la ubicación de trabajo de
# RabbitMQ (erlang cookie vía $HOME, y la base de mnesia vía
# RABBITMQ_MNESIA_BASE) hacia /var/lib/rabbitmq-runtime — un directorio
# nuevo, sin declaración VOLUME en ninguna capa, que esta imagen deja con el
# ownership correcto en tiempo de BUILD (no de arranque), así que Fargate no
# lo toca. Sin impacto en este despliegue: no necesita persistencia entre
# reinicios (confirmado — Postgres/Mongo pierden sus datos igual).
RUN mkdir -p /var/lib/rabbitmq-runtime \
    && chown -R rabbitmq:rabbitmq /var/lib/rabbitmq-runtime
ENV HOME=/var/lib/rabbitmq-runtime
ENV RABBITMQ_MNESIA_BASE=/var/lib/rabbitmq-runtime/mnesia
