#!/bin/bash

sudo apt update && sudo apt install curl
curl --location --remote-name https://github.com/Orange-OpenSource/hurl/releases/download/3.0.0/hurl_3.0.0_amd64.deb
sudo apt update && sudo apt install ./hurl_3.0.0_amd64.deb


kubectl --kubeconfig /home/runner/.kube/config get nodes -o wide

cluster_ip=$(kubectl --kubeconfig /home/runner/.kube/config get nodes -o jsonpath={.items[1].status.addresses[?\(@.type==\"InternalIP\"\)].address})

echo "============cluster_ip:${cluster_ip}============"
echo "============echo test============"
kubectl --kubeconfig /home/runner/.kube/config apply -f echo.yaml
kubectl --kubeconfig /home/runner/.kube/config wait --for=condition=Ready pod -l app=echo
sleep 5

cat>echo<<EOF 
GET http://${cluster_ip}:9000/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9000/get"
EOF
hurl --test echo -v

echo "============change config test============"
kubectl --kubeconfig /home/runner/.kube/config patch gateway gateway --type json -p='[{"op": "replace", "path": "/spec/listeners/0/port", "value": 9001}]'
sleep 1

cat>change-port<<EOF 
GET http://${cluster_ip}:9001/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9001/get"
EOF
hurl --test change-port -v

kubectl --kubeconfig /home/runner/.kube/config patch httproute echo --type json -p='[{"op": "replace", "path": "/spec/rules/0/matches/0/path/value", "value": "/hi"}]'
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

kubectl --kubeconfig /home/runner/.kube/config annotate --overwrite gateway gateway log_level="trace"
sleep 1

echo "kubectl logs -l app=spacegate -n spacegate"
kubectl --kubeconfig /home/runner/.kube/config logs -l app=spacegate -n spacegate

echo "============[gateway]tls test============"
openssl genrsa -out rsa_priv.key 2048
# ca
openssl req -new -x509 -key rsa_priv.key -out ca.crt -days 3650 -subj "/C=CN/ST=HangZhou/O=idealworld/OU=idealworld/CN=www.idealworld.group"
# csr
openssl req -new -key rsa_priv.key -out server.csr -subj "/C=CN/ST=HangZhou/O=idealworld/OU=idealworld/CN=www.idealworld.group"
#cert
openssl x509 -req -days 365 -in server.csr -CA ca.crt -CAkey rsa_priv.key -CAcreateserial -out cert.cert
secret_crt=$(cat cert.cert | base64 | sed ':a;N;$!ba;s/\n//g')
secret_key=$(cat rsa_priv.key | base64 | sed ':a;N;$!ba;s/\n//g')
cat>secret.yaml<<EOF
apiVersion: v1
kind: Secret
metadata:
  name: tls-secret
type: kubernetes.io/tls
data:
  tls.crt: ${secret_crt}
  tls.key: ${secret_key}
EOF
kubectl --kubeconfig /home/runner/.kube/config apply -f secret.yaml

kubectl --kubeconfig /home/runner/.kube/config get secret -o wide

kubectl --kubeconfig /home/runner/.kube/config patch gateway gateway --type json -p='[{"op": "replace", "path": "/spec/listeners/0/protocol", "value": "HTTPS"},{"op": "replace", "path": "/spec/listeners/0/tls", "value": {"mode": "Terminate","certificateRefs":[{"kind":"Secret","name":"tls-secret","namespace":"default"}]}}]'
sleep 1

cat>tls-test1.hurl<<EOF
GET https://${cluster_ip}:9001/echo/get

HTTP 200
[Asserts]
certificate "Subject" == "C=CN, ST=HangZhou, O=idealworld, OU=idealworld, CN=www.idealworld.group"
EOF
hurl --test tls-test1.hurl --insecure --verbose

cat>tls-test2.hurl<<EOF
GET https://${cluster_ip}:9001/hi/get

HTTP 200
[Asserts]
certificate "Subject" == "C=CN, ST=HangZhou, O=idealworld, OU=idealworld, CN=www.idealworld.group"
EOF
hurl --test tls-test2.hurl --insecure --verbose

echo "============[gateway]hostname test============"
cat>>/etc/hosts<<EOF
# test hostname
${cluster_ip} testhosts1
${cluster_ip} testhosts2
${cluster_ip} app.testhosts2
EOF

kubectl --kubeconfig /home/runner/.kube/config delete gateway gateway
kubectl --kubeconfig /home/runner/.kube/config apply -f echo.yaml
kubectl --kubeconfig /home/runner/.kube/config patch gateway gateway --type json -p='[{"op": "replace", "path": "/spec/listeners/0/hostname", "value": "testhosts1"}]'
sleep 5

cat>hostname_test.hurl<<EOF
GET http://testhosts1:9000/echo/get

HTTP 200
[Asserts]
header "content-length" != "0"
jsonpath "$.url" == "http://testhosts1:9000/get"

GET http://testhosts2:9000/echo/get

HTTP 404
[Asserts]
header "content-length" != "0"
jsonpath "$.msg" == "[SG] Hostname Not found"
EOF

hurl --test hostname_test.hurl -v

kubectl --kubeconfig /home/runner/.kube/config patch gateway gateway --type json -p='[{"op": "replace", "path": "/spec/listeners/0/hostname", "value": "*.testhosts2"}]'
sleep 1

cat>hostname_test2.hurl<<EOF
GET http://testhosts2:9000/echo/get

HTTP 404
[Asserts]
header "content-length" != "0"
jsonpath "$.msg" == "[SG] Hostname Not found"

GET http://app.testhosts2:9000/echo/get

HTTP 200
[Asserts]
header "content-length" != "0"
jsonpath "$.url" == "http://app.testhosts2:9000/get"
EOF

hurl --test hostname_test2.hurl -v

echo "============[gateway]multiple listeners test============"
kubectl --kubeconfig /home/runner/.kube/config apply -f mult_listeners.yaml
sleep 5

cat>mult_listeners.hurl<<EOF 
GET http://${cluster_ip}:9000/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9000/get"

GET http://${cluster_ip}:9100/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9100/get"
EOF
hurl --test mult_listeners.hurl -v


echo "============[gateway]redis connction test============"
kubectl --kubeconfig /home/runner/.kube/config delete gateway gateway
kubectl --kubeconfig /home/runner/.kube/config apply -f gateway_redis_test.yaml
kubectl --kubeconfig /home/runner/.kube/config wait --for=condition=Ready pod -l app=redis
kubectl --kubeconfig /home/runner/.kube/config annotate --overwrite gateway gateway redis_url="redis://redis-service.default:6379"
sleep 1

cat>redis.hurl<<EOF
GET http://${cluster_ip}:9000/echo/get

HTTP 200
[Asserts]
jsonpath "$.url" == "http://${cluster_ip}:9000/get"

EOF
hurl --test redis.hurl -v


echo "============[websocket]no backend test============"
kubectl --kubeconfig /home/runner/.kube/config delete gateway gateway
kubectl --kubeconfig /home/runner/.kube/config delete httproutes --all
wget https://github.com/vi/websocat/releases/download/v1.11.0/websocat.x86_64-unknown-linux-musl
chmod 770 websocat.x86_64-unknown-linux-musl

kubectl --kubeconfig /home/runner/.kube/config apply -f websocket_base_test.yaml
sleep 5

echo ====echo hi
echo hi|./websocat.x86_64-unknown-linux-musl "ws://${cluster_ip}:9000"

echo "kubectl logs -l app=spacegate -n spacegate"
kubectl --kubeconfig /home/runner/.kube/config logs -l app=spacegate -n spacegate

command_output=$(echo hi | ./websocat.x86_64-unknown-linux-musl "ws://${cluster_ip}:9000")

expected_output=""

if [ "$command_output" = "$expected_output" ]; then
    echo "Output matches the expected value."
else
    echo "Output does not match the expected value."
    exit 1
fi

echo "============[websocket]basic test============"
kubectl --kubeconfig /home/runner/.kube/config apply -f websocket_base_test.yaml
kubectl --kubeconfig /home/runner/.kube/config apply -f websocket_echo_test.yaml

kubectl --kubeconfig /home/runner/.kube/config wait --for=condition=Ready pod -l app=websocket-echo
sleep 5

echo ====echo hi
echo hi|./websocat.x86_64-unknown-linux-musl "ws://${cluster_ip}:9000"

command_output=$(echo hi | ./websocat.x86_64-unknown-linux-musl "ws://${cluster_ip}:9000")

expected_output="hi"

 if [ "$command_output" = "$expected_output" ]; then
     echo "Output matches the expected value."
 else
     echo "Output does not match the expected value."
     exit 1
 fi

echo "============[httproute]hostnames test============"
kubectl --kubeconfig /home/runner/.kube/config delete httproutes --all
kubectl --kubeconfig /home/runner/.kube/config delete gateway gateway
cat>>/etc/hosts<<EOF
${cluster_ip} testhosts1.httproute
${cluster_ip} testhosts2.httproute
${cluster_ip} app.testhosts2.httproute
EOF

kubectl --kubeconfig /home/runner/.kube/config apply -f echo.yaml
kubectl --kubeconfig /home/runner/.kube/config patch httproute echo --type json -p='[{"op": "add", "path": "/spec/hostnames", "value": "testhosts1.httproute"}]'
sleep 1

echo =====
kubectl --kubeconfig /home/runner/.kube/config describe httproute echo

cat>hostname_test.hurl<<EOF
GET http://testhosts1.httproute:9000/echo/get

HTTP 200
[Asserts]
header "content-length" != "0"
jsonpath "$.url" == "http://testhosts1.httproute:9000/get"

GET http://testhosts2.httproute:9000/echo/get

HTTP 404
[Asserts]
header "content-length" != "0"
jsonpath "$.msg" == "[SG] Hostname Not found"
EOF

hurl --test hostname_test.hurl -v

kubectl --kubeconfig /home/runner/.kube/config patch gateway gateway --type json -p='[{"op": "replace", "path": "/spec/listeners/0/hostname", "value": "*.testhosts2.httproute"}]'
sleep 1

cat>hostname_test2.hurl<<EOF
GET http://testhosts2.httproute:9000/echo/get

HTTP 404
[Asserts]
header "content-length" != "0"
jsonpath "$.msg" == "[SG] Hostname Not found"

GET http://app.testhosts2.httproute:9000/echo/get

HTTP 200
[Asserts]
header "content-length" != "0"
jsonpath "$.url" == "http://app.testhosts2.httproute:9000/get"
EOF

hurl --test hostname_test2.hurl -v

echo "============[httproute]rule match test============"
echo "============[httproute]timeout test============"
echo "============[httproute]backend with k8s service test============"
echo "============[httproute]backend weight test============"
echo "============[filter]routing level test============"
kubectl --kubeconfig /home/runner/.kube/config delete httproutes --all
kubectl --kubeconfig /home/runner/.kube/config delete gateway gateway
kubectl --kubeconfig /home/runner/.kube/config apply -f echo.yaml
kubectl --kubeconfig /home/runner/.kube/config apply -f filter_gateway_test.yaml
sleep 5

cat>filter_routing.hurl<<EOF
GET http://${cluster_ip}:8110

HTTP 200
[Asserts]

EOF
hurl --test filter_routing.hurl -v
echo "============[filter]global level test============"
kubectl --kubeconfig /home/runner/.kube/config delete sgfilters --all
kubectl --kubeconfig /home/runner/.kube/config apply -f echo.yaml
kubectl --kubeconfig /home/runner/.kube/config apply -f filter_gateway_test.yaml
sleep 5

curl http://${cluster_ip}:8110

cat>filter_global.hurl<<EOF
GET http://${cluster_ip}:8110

HTTP 200
[Asserts]

EOF
hurl --test filter_global.hurl -v

echo "============[filter]multiple levels test============"
