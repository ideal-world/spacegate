FROM ubuntu

RUN apt-get update
RUN apt install -y --reinstall ca-certificates
RUN update-ca-certificates -f
RUN  echo 'ca_certificate=/etc/ssl/certs/ca-certificates.crt'  >> /root/.wgetrc 

COPY ./spacegate ./spacegate

CMD ["./spacegate"]