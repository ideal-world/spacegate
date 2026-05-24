# TC-MET-01 / TC-DEP-02
GET {{service_url}}/healthz

HTTP 200
[Asserts]
body == "ok"

GET {{service_url}}/metrics

HTTP 200
[Asserts]
body contains "queue_depth"
body contains "pel_size"
