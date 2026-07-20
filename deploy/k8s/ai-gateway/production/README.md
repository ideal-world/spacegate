# AI Gateway 生产部署

本目录是运维唯一需要应用的 AI Gateway 业务清单。它部署排队限流服务、AI 路由和 SgFilter；不部署 Demo 上游、Redis、旧 Admin UI 或 HTTP Wasm 分发服务。`deploy/k8s/ai-gateway/` 根目录中的其他清单仅用于示例和测试，不得在生产环境执行 `kubectl apply -k`。

## 前置条件

1. `spacegate` namespace、Gateway API、SgFilter CRD、SpaceGate DaemonSet 已部署。
2. SpaceGate 镜像必须以 `build-k8s,wasm,dylib` feature 构建。
3. 外部 Redis 已可从 `spacegate` namespace 访问，版本为 Redis 7 或以上。
4. 已存在真实模型上游 Kubernetes Service；将 `httproute-ai.yaml` 中的 `ai-model-upstream:8080` 改为实际 Service 和端口。
5. SpaceGate 镜像已内置 `ai-gateway-queue` Wasm；镜像必须由 `resource/docker/spacegate-k8s/Dockerfile` 或包含同等复制步骤的生产 Dockerfile 构建。
6. 将 `resource/kube-manifests/spacegate-gateway.yaml` 中的 `registry.example.com/spacegate/spacegate:REPLACE_ME` 替换为已启用 `build-k8s,wasm,dylib` feature 的不可变镜像 tag 或 digest。

## 一次性准备

滚动 SpaceGate DaemonSet 后，在任一 Pod 内校验系统 Wasm 制品：

```bash
kubectl rollout status daemonset/spacegate -n spacegate --timeout=180s
kubectl exec -n spacegate daemonset/spacegate -- \
  test -r /lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
```

创建 Redis Secret 和运行参数 ConfigMap。不要将真实 Redis 密码提交到 Git：

```bash
kubectl -n spacegate create secret generic ai-gateway-queue-redis \
  --from-literal=REDIS_URL='rediss://:<password>@<redis-host>:6379/<db>' \
  --dry-run=client -o yaml | kubectl apply -f -

kubectl -n spacegate create configmap ai-gateway-queue-runtime \
  --from-literal=AI_UPSTREAM_BASE_URL='https://<model-provider-or-internal-upstream>' \
  --from-literal=AI_ADMIN_CORS_ORIGINS='https://<admin-domain>' \
  --dry-run=client -o yaml | kubectl apply -f -
```

`AI_UPSTREAM_BASE_URL` 供队列 worker 处理异步和 wait 请求使用；为空时任务只会存入 Redis，不会被 worker 执行。

## 部署

1. 在 `ai-gateway-service.yaml` 中将 `ai-gateway-service:REPLACE_ME` 改为不可变镜像标签或 digest。
2. 在 `httproute-ai.yaml` 中确认真实模型 Service 名称与端口，并将 `parentRefs.name` 改为现有 SpaceGate Gateway 名称。
3. 默认生产包复用既有 Gateway，不创建 listener；渲染确认后再应用：

```bash
cd deploy/k8s/ai-gateway/production
kubectl kustomize .
kubectl apply -k .
kubectl rollout status deployment/ai-gateway-queue -n spacegate --timeout=180s
```

只有在集群没有可复用的 SpaceGate Gateway，且确认节点端口 `9993` 未被使用时，才部署专用 Gateway：

```bash
cd deploy/k8s/ai-gateway/production-with-dedicated-gateway
kubectl kustomize .
kubectl apply -k .
```

## 验证与回滚

```bash
kubectl get deploy,svc,gateway,httproute,sgfilter -n spacegate
kubectl get pods -n spacegate -l app.kubernetes.io/name=ai-gateway-queue
kubectl exec -n spacegate daemonset/spacegate -- \
  test -r /lib/spacegate/wasm/spacegate_plugin_ai_gateway_queue.wasm
kubectl logs -n spacegate deployment/ai-gateway-queue --tail=200
```

回滚业务服务镜像：

```bash
kubectl rollout undo deployment/ai-gateway-queue -n spacegate
kubectl rollout status deployment/ai-gateway-queue -n spacegate --timeout=180s
```

回滚或停用排队限流时删除 SgFilter 即可；HTTPRoute 和上游服务不受影响：

```bash
kubectl delete -n spacegate -f sgfilter-ai-gateway-queue.yaml
```
