# make k8s docker
```shell
DOCKER_REPO=ecfront DOCKER_VERSION=v1.0 cargo make build-k8s-docker 
```

or 

```shell
cargo make build-k8s-docker --env-file release
```


