FROM mineman/ubuntu:openssl1.1.1t

# When error: `libssl. so.1.1: cannot open shared object file: No such file or directory` appears, please use the following statement to install libssl
# RUN apt update && apt install -y wget && wget http://nz2.archive.ubuntu.com/ubuntu/pool/main/o/openssl/libssl1.1_1.1.1f-1ubuntu2.19_amd64.deb && dpkg -i libssl1.1_1.1.1f-1ubuntu2.19_amd64.deb
RUN apt update && apt install -y --reinstall ca-certificates
RUN update-ca-certificates -f
RUN  echo 'ca_certificate=/etc/ssl/certs/ca-certificates.crt'  >> /root/.wgetrc 

COPY ./spacegate ./spacegate

CMD ["./spacegate"]