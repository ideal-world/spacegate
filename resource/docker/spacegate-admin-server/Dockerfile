FROM nginx:latest
LABEL org.opencontainers.image.source https://github.com/ideal-world/spacegate

WORKDIR /usr/src/app

COPY ./dist /usr/share/nginx/html

COPY ./spacegate-admin-server /usr/src/app/admin-server
COPY nginx-default.conf /etc/nginx/nginx.conf
COPY start.sh /usr/src/app/start.sh
RUN chmod +x start.sh

# COPY libssl1.1_1.1.1f-1ubuntu2.20_amd64.deb /usr/src/app
# RUN  dpkg -i libssl1.1_1.1.1f-1ubuntu2.20_amd64.deb

EXPOSE 9080

CMD ["sh", "-c", "sh start.sh"]