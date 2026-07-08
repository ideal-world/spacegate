# TC-RL-02 / TC-HDR-03：限流 check 与缺 tenant
POST {{service_url}}/v1/ratelimit/check
X-Tenant-Id: hurl-tenant-a
X-RateLimit-Policy: abandon
X-Original-Path: /v1/chat

HTTP 200
[Asserts]
jsonpath "$.allowed" == true
jsonpath "$.retry_after_ms" == 0

POST {{service_url}}/v1/ratelimit/check
X-RateLimit-Policy: abandon

HTTP 400
