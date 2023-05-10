FROM rancher/k3d:latest-dind

RUN apt update
RUN apt install -y curl

# Install kubectl
RUN curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl"
RUN install -o root -g root -m 0755 kubectl /usr/local/bin/kubectl

# Install Gateway API resources
RUN kubectl apply -f https://github.com/kubernetes-sigs/gateway-api/releases/download/v0.6.2/experimental-install.yaml
