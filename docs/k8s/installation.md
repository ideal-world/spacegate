# Installation

This guide walks you through how to install Spacegate Kubernetes Gateway on a generic Kubernetes cluster.

## Prerequisites

- [kubectl](https://kubernetes.io/docs/tasks/tools/)

## Deploy Spacegate Kubernetes Gateway

> Note: Spacegate Kubernetes Gateway can only run in the `spacegate` namespace.

1. Install the Gateway API resources from the standard channel (the CRDs and the validating webhook):

    ```
    kubectl apply -f https://github.com/kubernetes-sigs/gateway-api/releases/download/v0.6.2/standard-install.yaml
    ```

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

1. Create the spacegate Namespace:

    ```
    kubectl apply -f https://raw.githubusercontent.com/ideal-world/spacegate/master/kernel/res/namespace.yaml
    ```

1. Create the GatewayClass resource:

    ```
    kubectl apply -f https://raw.githubusercontent.com/ideal-world/spacegate/master/kernel/res/gatewayclass.yaml
    ```

1. Create the Spacegate Kubernetes CRD HttpSpaceroute:

    ```
    kubectl apply -f https://raw.githubusercontent.com/ideal-world/spacegate/master/kernel/res/spacegate-httproute.yaml
    ```

1. Deploy the Spacegate Kubernetes Gateway:

    ```
    kubectl apply -f https://raw.githubusercontent.com/ideal-world/spacegate/master/kernel/res/spacegate-gateway.yaml
    ```

1. Confirm the Spacegate Kubernetes Gateway is running in `spacegate` namespace:

    ```
    kubectl get pods -n spacegate
    NAME             READY   STATUS    RESTARTS   AGE
    spacegate-xxxx   1/1     Running   0          10s
    ```

### Use Spacegate Kubernetes Gateway

To get started, follow the tutorials in the `examples/k8s-*` directory.