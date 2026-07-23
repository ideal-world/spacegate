# Development

## Environmental preparation

1. Install kubectl

    ```
    curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl"
    sudo install -o root -g root -m 0755 kubectl /usr/local/bin/kubectl
    ```

1. Install k3d

    ```
    wget -q -O - https://raw.githubusercontent.com/k3d-io/k3d/main/install.sh | bash
    k3d cluster create spacegate-test --no-lb --kubeconfig-update-default
    ```

1. Install Gateway API resources

    ```
    kubectl apply -f https://github.com/kubernetes-sigs/gateway-api/releases/download/v0.6.2/experimental-install.yaml
    ```

    OR

    ```
    kubectl apply -f ./resource/kube-manifests/gateway-api-0.6.2-experimental-china.yaml
    ```

    * This file replaces the addresses of the two images to solve the problem of inaccessibility in mainland China

1. Confirm the Kubernetes Gateway is running:

    ```
    kubectl get deploy -n gateway-system
    NAME                           READY   UP-TO-DATE   AVAILABLE   AGE
    gateway-api-admission-server   1/1     1            1           10s
    ```

## Process spacegate image

1. Build image

    ```
    cargo build --release -p spacegate --features build-k8s,wasm,dylib,static-openssl
    cp target/release/spacegate resource/docker/spacegate/spacegate
    docker build -t spacegate:dev resource/docker/spacegate
    k3d image import spacegate:dev -c spacegate-test
    ```

    Native `.so` plugins built into the image should live under `/lib/spacegate/plugins`. Plugins mounted by Kubernetes should use `/var/lib/spacegate/plugins` so they do not shadow image-bundled plugins.

## Process spacegate resources

1. Import kubeconfig to Secret(Optional, this step is not required if using the default `hostNetwork: true` ):

    ```
    export kubeconfig=`cat $HOME/.kube/config | base64 -w 0`
    cat <<EOF | kubectl apply -f -
    apiVersion: v1
    kind: Secret
    metadata:
      name: kubeconfig
      namespace: spacegate
    data:
      config: $kubeconfig
    EOF
    ```

1. Install spacegate resources

    ```
    cd ../../../
    kubectl apply -f ./resource/kube-manifests/namespace.yaml
    kubectl apply -f ./resource/kube-manifests/spacegate-httproute.yaml
    kubectl apply -f ./resource/kube-manifests/spacegate-mcproute.yaml
    kubectl apply -f ./resource/kube-manifests/higress-wasmplugin-crd.yaml
    kubectl apply -f ./resource/kube-manifests/gatewayclass.yaml
    kubectl apply -f ./resource/kube-manifests/spacegate-gateway.yaml
    kubectl set image daemonset/spacegate spacegate=spacegate:dev -n spacegate
    kubectl rollout status daemonset/spacegate -n spacegate
    ```

1. Confirm the spacegate resources is running in `spacegate` namespace:

    ```
    kubectl get pods -n spacegate
    NAME             READY   STATUS    RESTARTS   AGE
    spacegate-xxxx   1/1     Running   0          10s
    ```

1. Forward gateway port to host(Optional)
    > https://github.com/k3d-io/k3d/issues/89

    ```
    docker run \
    -d \
    -p 9002:9002 \
    --name=k3d-default-server-9002-link \
    --network k3d-spacegate-test \
    --rm \
    alpine/socat \
      TCP4-LISTEN:9002,fork,reuseaddr \
      TCP4:k3d-spacegate-test-server-0:9000
    ```

    Now,you can use 127.0.0.1:9002 to access spacegate
