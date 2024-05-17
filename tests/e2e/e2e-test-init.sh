cd ~
pwd
cd -
echo $KUBECONFIG
echo "========="
cat ~/.kube/config
kubectl apply -f https://github.com/kubernetes-sigs/gateway-api/releases/download/v0.6.2/experimental-install.yaml
cargo build --bin spacegate --features build-k8s
cd resource/docker/spacegate
mv ../../../target/debug/spacegate ./
docker build -t ecfront/spacegate:latest .
rm spacegate
k3d image import ecfront/spacegate:latest -c spacegate-test --verbose

kubectl wait --for=condition=Ready pod -l name=gateway-api-admission-server -n gateway-system
sleep 10
cd ../../../
kubectl apply -f ./resource/kube-manifests/namespace.yaml
kubectl apply -f ./resource/kube-manifests/gatewayclass.yaml
kubectl apply -f ./resource/kube-manifests/spacegate-gateway.yaml
kubectl apply -f ./resource/kube-manifests/spacegate-httproute.yaml
sleep 5
kubectl wait --for=condition=Ready pod -l app=spacegate -n spacegate