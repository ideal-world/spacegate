FROM nginx:latest

WORKDIR /usr/src/app

COPY ./dist /usr/share/nginx/html

COPY ./conf-default.toml /usr/src/app/config/conf-default.toml
COPY ./spacegate-admin /usr/src/app/spacegate-admin

COPY nginx-default.conf /etc/nginx/nginx.conf
COPY start.sh /usr/src/app/start.sh
RUN chmod +x start.sh

# COPY libssl1.1_1.1.1f-1ubuntu2.20_amd64.deb /usr/src/app
# RUN  dpkg -i libssl1.1_1.1.1f-1ubuntu2.20_amd64.deb


EXPOSE 9080


CMD ["sh", "-c", "sh start.sh"]
