#!/bin/sh
./admin-server -p 9081 -c $CONFIG &
pid="$!"
nginx -g 'daemon off;'
kill -TERM "$pid"
exit 143