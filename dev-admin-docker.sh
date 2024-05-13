cargo build -p spacegate-admin-server
mv ./target/debug/spacegate-admin-server ./resource/docker/spacegate-admin-server
cd ./resource/docker/spacegate-admin-server
docker build -t 172.30.84.225:443/idp-prod/spacegate-admin-server:v0.2.4  .
docker push 172.30.84.225:443/idp-prod/spacegate-admin-server:v0.2.4