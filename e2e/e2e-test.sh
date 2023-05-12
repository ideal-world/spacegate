#!/bin/bash

sudo apt update && sudo apt install curl
curl --location --remote-name https://github.com/Orange-OpenSource/hurl/releases/download/3.0.0/hurl_3.0.0_amd64.deb
sudo apt update && sudo apt install ./hurl_3.0.0_amd64.deb

echo `kubectl get nodes -o wide`

cluster_ip=`kubectl get nodes -o jsonpath={.items[1].status.addresses[?\(@.type==\"InternalIP\"\)].address}`

echo "============echo test============"
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

echo "============change config test============"
kubectl patch gateway gateway --type json -p='[{"op": "replace", "path": "/spec/listeners/0/port", "value": 9001}]'
sleep 1

cat>change-port<<EOF 
GET http://${cluster_ip}:9001/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9001/get"
EOF
hurl --test change-port -v

kubectl patch httproute echo --type json -p='[{"op": "replace", "path": "/spec/rules/0/matches/0/path/value", "value": "/hi"}]'
sleep 1

cat>change-route<<EOF 
GET http://${cluster_ip}:9001/echo/get

HTTP 200
[Asserts]
header "content-length" == "0"

GET http://${cluster_ip}:9001/hi/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9001/get"
EOF
hurl --test change-route -v

# TODO
echo "============[gateway]tls test============"
echo "============[gateway]multiple listeners test============"
echo "============[gateway]hostname test============"
echo "============[gateway]redis connction test============"
echo "============[websocket]no backend test============"
echo "============[websocket]basic test============"
echo "============[httproute]hostnames test============"
echo "============[httproute]rule match test============"
echo "============[httproute]timeout test============"
echo "============[httproute]backend with k8s service test============"
echo "============[httproute]backend weight test============"
echo "============[filter]backend level test============"
echo "============[filter]rule level test============"
echo "============[filter]routing level test============"
echo "============[filter]global level test============"
echo "============[filter]multiple levels test============"
