FROM ubuntu

# RUN apt update
RUN apt update && apt install -y wget && wget http://nz2.archive.ubuntu.com/ubuntu/pool/main/o/openssl/libssl1.1_1.1.1f-1ubuntu2.19_amd64.deb && dpkg -i libssl1.1_1.1.1f-1ubuntu2.19_amd64.deb
RUN apt update && apt install -y --reinstall ca-certificates
RUN update-ca-certificates -f
RUN  echo 'ca_certificate=/etc/ssl/certs/ca-certificates.crt'  >> /root/.wgetrc 

COPY ./spacegate ./spacegate

CMD ["./spacegate"]