
<!-- ## Install k3s

```
curl -sfL https://get.k3s.io | sh -
```
OR
```
curl -sfL https://rancher-mirror.rancher.cn/k3s/k3s-install.sh | INSTALL_K3S_MIRROR=cn sh -
``` -->

## Install k3d

```
wget -q -O - https://raw.githubusercontent.com/k3d-io/k3d/main/install.sh | bash

k3d cluster create spacegate-test --no-lb

k3d kubeconfig write spacegate-test
```

## Install kubectl

```
curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl"
sudo install -o root -g root -m 0755 kubectl /usr/local/bin/kubectl
```

<!-- ## k3s permission denied when using kubectl

```
sudo cp /etc/rancher/k3s/k3s.yaml ~/.kube/config && chown $USER ~/.kube/config && chmod 600 ~/.kube/config && export KUBECONFIG=~/.kube/config
``` -->

## Use local images(example)

```
cargo build -p spacegate
cd services/full/res
mv ../../../target/release/spacegate ./
docker build -t ecfront/spacegate:0.1.0-alpha.2 .
k3d image import ecfront/spacegate:0.1.0-alpha.2 -c spacegate-test
```



