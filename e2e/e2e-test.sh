#!/bin/bash

apt update
apt install libxml2-dev
cargo install hurl

echo `kubectl get nodes -o wide`

cluster_ip=`kubectl get nodes -o jsonpath={.items[1].status.addresses[?\(@.type==\"InternalIP\"\)].address}`

echo "===echo test==="
kubectl apply -f echo.yaml
kubectl wait --for=condition=Ready pod -l app=echo

cat>echo<<EOF 
GET http://${cluster_ip}:9000/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9000/get"
EOF
hurl --test echo -v

# echo "===change port==="
# kubectl patch service nginx --type json -p='[{"op": "replace", "path": "/spec/type/spec/ports/0/targetPort", "new port"}]' && \
