./static-web-server --port 8080 --root ./static-server
./spacegate ./config

sleep 5

wrk -t88 -c10000 -d20s "http://127.0.0.1:8080/index.html"