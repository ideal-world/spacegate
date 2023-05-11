#!/bin/bash

sudo apt update && sudo apt install curl
curl --location --remote-name https://github.com/Orange-OpenSource/hurl/releases/download/3.0.0/hurl_3.0.0_amd64.deb
sudo apt update && sudo apt install ./hurl_3.0.0_amd64.deb

echo `kubectl get nodes -o wide`

cluster_ip=`kubectl get nodes -o jsonpath={.items[1].status.addresses[?\(@.type==\"InternalIP\"\)].address}`

echo "===echo test==="
kubectl apply -f echo.yaml
kubectl wait --for=condition=Ready pod -l app=echo
sleep 5

cat>echo<<EOF 
GET http://${cluster_ip}:9000/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9000/get"
EOF
hurl --test echo -v

echo "===change config==="
kubectl patch gateway gateway --type json -p='[{"op": "replace", "path": "/spec/listeners/0/port", "value": 9001}]'

cat>change-port<<EOF 
GET http://${cluster_ip}:9001/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9001/get"
EOF
hurl --test change-port -v

kubectl patch httproute echo --type json -p='[{"op": "replace", "path": "/spec/rules/0/matches/0/path/value", "value": "/hi"}]'

cat>change-route<<EOF 
GET http://${cluster_ip}:9001/echo/get

HTTP 404

GET http://${cluster_ip}:9001/hi/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9001/get"
EOF
hurl --test change-route -v
