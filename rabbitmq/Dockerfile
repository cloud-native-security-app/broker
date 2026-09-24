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
