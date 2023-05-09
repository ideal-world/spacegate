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
    kubectl apply -f ./docs/k8s/gateway-api-0.6.2-experimental-china.yaml
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
    cargo build --release -p spacegate
    cd services/full/res
    mv ../../../target/release/spacegate ./
    docker build -t ecfront/spacegate:0.1.0-alpha.2 .
    rm spacegate
    k3d image import ecfront/spacegate:0.1.0-alpha.2 -c spacegate-test
    ```

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
kubectl apply -f ./kernel/res/namespace.yaml
kubectl apply -f ./kernel/res/gatewayclass.yaml
kubectl apply -f ./kernel/res/spacegate-gateway.yaml
```

1. Confirm the spacegate resources is running in `spacegate` namespace:

```
kubectl get pods -n spacegate
NAME             READY   STATUS    RESTARTS   AGE
spacegate-xxxx   1/1     Running   0          10s
```