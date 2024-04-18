#!/bin/sh
./admin-server -p 9081 -c $CONFIG -s $SCHEMA --host 0.0.0.0 &
pid="$!"
nginx -g 'daemon off;'
kill -TERM "$pid"
exit 143