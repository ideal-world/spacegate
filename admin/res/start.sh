#!/bin/sh
./spacegate-admin &
pid="$!"
nginx -g 'daemon off;'
kill -TERM "$pid"
exit 143