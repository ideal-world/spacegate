#!/bin/sh
./admin-server -p 9081 -c $CONFIG -s $SCHEMA &
pid="$!"
nginx -g 'daemon off;'
kill -TERM "$pid"
exit 143