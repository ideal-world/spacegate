# TC-RL-05 Admin 租户规则
PUT {{service_url}}/v1/admin/tenant-rate-limits
Content-Type: application/json
```
{
  "tenant": "hurl-admin",
  "rps": 1,
  "burst": 1,
  "cost": 1
}
```

HTTP 200
[Asserts]
jsonpath "$.tenant" == "hurl-admin"

POST {{service_url}}/v1/ratelimit/check
X-Tenant-Id: hurl-admin
X-RateLimit-Policy: abandon

HTTP 200
[Asserts]
jsonpath "$.allowed" == true

POST {{service_url}}/v1/ratelimit/check
X-Tenant-Id: hurl-admin
X-RateLimit-Policy: abandon

HTTP 200
[Asserts]
jsonpath "$.allowed" == false
