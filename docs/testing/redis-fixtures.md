- 明白，下面是给 `hai-hub` 原生插件链使用的 Redis Mock 数据，不是旧 Wasm 的 HTTP Mock。

  请在 DbGate 的 Redis 查询窗口执行。若 DbGate 不支持一次执行多条，逐条执行每个 `SET`。

  ```
  SET hai:apikey:demo-key '{"app_id":"demo-app","asset_ids":["demo-skill","demo-api","quota-api"],"allow_ips":[],"deny_ips":[],"allow_mac_addrs":[],"deny_mac_addrs":[],"expired_at":"2099-01-01T00:00:00Z"}'
  
  SET hai:asset:demo-skill '{"asset_id":"demo-skill","asset_type":"skill","asset_status":"published","asset_content":"# demo skill\n\nHAI native plugin chain is working.","asset_secret_params":[],"asset_secret_values":{},"allowed_output_targets":[]}'
  
  SET hai:asset:demo-api '{"asset_id":"demo-api","asset_type":"api","asset_status":"published","runtime_endpoint":"https://httpbin.org/anything/demo-api","runtime_endpoint_method":["POST"],"max_concurrent":100,"qps_limit":1000,"timeout_sec":30,"asset_secret_params":[],"asset_secret_values":{},"allowed_output_targets":[]}'
  
  SET hai:asset:quota-api '{"asset_id":"quota-api","asset_type":"api","asset_status":"published","runtime_endpoint":"https://httpbin.org/anything/quota-api","runtime_endpoint_method":["POST"],"max_concurrent":1,"qps_limit":1,"timeout_sec":30,"asset_secret_params":[],"asset_secret_values":{},"allowed_output_targets":[]}'
  
  SET hai:asset:demo-skill:version:v1 '{"asset_id":"demo-skill","asset_type":"skill","asset_status":"published","asset_content":"# demo skill v1\n\nVersioned asset loaded from Redis.","asset_secret_params":[],"asset_secret_values":{},"allowed_output_targets":[]}'
  
  SET hai:asset:demo-skill:version:v2 '{"asset_id":"demo-skill","asset_type":"skill","asset_status":"published","asset_content":"# demo skill v2\n\nVersioned asset loaded from Redis.","asset_secret_params":[],"asset_secret_values":{},"allowed_output_targets":[]}'
  
  SET hai:apikey:demo-key '{"app_id":"demo-app","asset_ids":["demo-skill","demo-api","quota-api","demo-internal-model"],"allow_ips":[],"deny_ips":[],"allow_mac_addrs":[],"deny_mac_addrs":[],"expired_at":"2099-01-01T00:00:00Z"}'
  
  SET hai:asset:demo-internal-model '{"asset_id":"demo-internal-model","asset_type":"model","asset_status":"published","runtime_endpoint":"http://model-gateway.example.internal:8080/v1/chat/completions","runtime_endpoint_method":["POST"],"max_concurrent":10,"qps_limit":20,"timeout_sec":120,"asset_secret_params":[{"secret_key":"MODEL_API_KEY","type":"header","name":"Authorization","required":true}],"asset_secret_values":{"MODEL_API_KEY":"Bearer REPLACE_WITH_REAL_INTERNAL_API_KEY"},"allowed_output_targets":[]}'
  ```

  验证键是否写入：

  ```
  GET hai:apikey:demo-key
  GET hai:asset:demo-skill
  GET hai:asset:demo-skill:version:v1
  ```

  推荐先测不依赖外网的直接响应资产：

  ```
  curl -i -X POST http://<网关地址>:9993/api/v1/skill/demo-skill \
    -H 'Hai-Api-Key: demo-key' \
    -d '{}'
  ```

  带版本测试：

  ```
  curl -i -X POST http://<网关地址>:9993/api/v1/skill/demo-skill \
    -H 'Hai-Api-Key: demo-key' \
    -H 'Hai-Asset-Version: v1' \
    -d '{}'
  
  curl -i -X POST http://<网关地址>:9993/api/v1/model/demo-internal-model \
    -H 'Hai-Api-Key: demo-key' \
    -H 'Content-Type: application/json' \
    -d '{"model":"your-model-name","messages":[{"role":"user","content":"hello"}],"stream":false}'
  ```

  预期分别返回 Redis 中的 `asset_content`，可验证 `hai-auth → hai-asset → hai-dispatch` 已生效。

  `demo-api`、`quota-api` 需要网关能够访问 `httpbin.org`；其中 `quota-api` 的 `qps_limit=1`、`max_concurrent=1` 可用于验证 `hai-quota` 限流。

  这些 key 与原生插件实际读取规则一致：

  ```
  hai:apikey:<api-key>
  hai:asset:<asset-id>
  hai:asset:<asset-id>:version:<version>
  ```

  还要确认 Gateway 的 `redis_url` 或 `hai-auth` / `hai-asset` 插件自身的 `redis_url` 指向 DbGate 连接的同一个 Redis DB。