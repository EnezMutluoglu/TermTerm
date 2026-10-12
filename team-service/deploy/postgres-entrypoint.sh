#!/bin/bash
set -euo pipefail
install -d -o postgres -g postgres -m 700 /run/termterm-tls
install -o postgres -g postgres -m 600 /run/tls-source/server.key /run/termterm-tls/server.key
install -o postgres -g postgres -m 644 /run/tls-source/server.crt /run/termterm-tls/server.crt
exec /usr/local/bin/docker-entrypoint.sh postgres \
  -c ssl=on -c ssl_cert_file=/run/termterm-tls/server.crt \
  -c ssl_key_file=/run/termterm-tls/server.key \
  -c hba_file=/etc/termterm-pg-hba.conf
