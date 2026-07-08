# TC-Q-02 / TC-HDR-04 / TC-HDR-05
POST {{service_url}}/v1/queue/enqueue
X-Tenant-Id: hurl-queue
X-RateLimit-Policy: queue
X-Callback-URL: {{callback_url}}
Content-Type: application/json
file,small.json;

HTTP 202
[Asserts]
header "X-Job-Id" exists
jsonpath "$.status" == "queued"
jsonpath "$.poll_url" matches "/jobs/.+/status"

POST {{service_url}}/v1/queue/enqueue
X-Tenant-Id: hurl-queue
X-RateLimit-Policy: queue
Content-Type: application/json
file,small.json;

HTTP 400
