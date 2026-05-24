# TC-W-02：wait 成功
POST {{service_url}}/v1/queue/enqueue-and-wait
X-Tenant-Id: hurl-wait
X-RateLimit-Policy: wait
X-Request-Timeout: 10
X-Original-Method: POST
X-Original-Path: /v1/chat
Content-Type: application/json
file,small.json;

HTTP 200
[Asserts]
header "X-Job-Id" exists
header "X-Queue-Wait-Ms" exists
jsonpath "$.upstream" == true
