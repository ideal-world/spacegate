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

1. Clone the repo and change into the `spacegate` directory:

   ```
   git clone https://github.com/idealworld/spacegate.git
   cd spacegate/kernel/res
   ```

1. Create the spacegate Namespace:

    ```
    kubectl apply -f namespace.yaml
    ```

1. Create the GatewayClass resource:

    ```
    kubectl apply -f gatewayclass.yaml
    ```

1. Deploy the Spacegate Kubernetes Gateway:

   ```
   kubectl apply -f spacegate-gateway.yaml
   ```

1. Confirm the Spacegate Kubernetes Gateway is running in `spacegate` namespace:

   ```
   kubectl get pods -n spacegate
   NAME             READY   STATUS    RESTARTS   AGE
   spacegate-xxxx   2/2     Running   0          112s
   ```

### Use Spacegate Kubernetes Gateway

To get started, follow the tutorials in the [examples/k8s](../../examples/k8s) directory.