#!/usr/bin/env bash
# Disposable TermTerm containers only. Certificates and wallet password stay in .lab.
set -euo pipefail
cd "$(dirname "$0")/.."
umask 077
mkdir -p .lab/database/tls
tls=$(realpath .lab/database/tls)
if [ ! -f "$tls/ca.pem" ]; then
  openssl req -x509 -newkey rsa:3072 -nodes -sha256 -days 365 -subj '/CN=TermTerm disposable DB CA' -keyout "$tls/ca.key" -out "$tls/ca.pem" 2>/dev/null
fi
if [ ! -f "$tls/server.pem" ]; then
  openssl req -new -newkey rsa:3072 -nodes -subj '/CN=localhost' -keyout "$tls/server.key" -out "$tls/server.csr" 2>/dev/null
  printf 'subjectAltName=DNS:localhost\nextendedKeyUsage=serverAuth\n' > "$tls/server.ext"
  openssl x509 -req -in "$tls/server.csr" -CA "$tls/ca.pem" -CAkey "$tls/ca.key" -CAcreateserial -days 365 -sha256 -extfile "$tls/server.ext" -out "$tls/server.pem" 2>/dev/null
fi
docker cp "$tls/server.pem" termterm-db-mssql:/var/opt/mssql/server.pem
docker cp "$tls/server.key" termterm-db-mssql:/var/opt/mssql/server.key
docker exec -u root termterm-db-mssql sh -c 'chown mssql:mssql /var/opt/mssql/server.*; chmod 600 /var/opt/mssql/server.key; /opt/mssql/bin/mssql-conf set network.tlscert /var/opt/mssql/server.pem; /opt/mssql/bin/mssql-conf set network.tlskey /var/opt/mssql/server.key; /opt/mssql/bin/mssql-conf set network.tlsprotocols 1.2'
docker restart termterm-db-mssql >/dev/null
# Native Oracle wallet: Oracle key-bag attributes are not preserved by generic PKCS12 imports.
if [ ! -f "$tls/wallet-password" ]; then openssl rand -hex 24 > "$tls/wallet-password"; fi
docker exec -u root termterm-db-oracle mkdir -p /opt/oracle/oradata/termterm-tls
docker cp "$tls/wallet-password" termterm-db-oracle:/opt/oracle/oradata/termterm-tls/password
docker exec -u root termterm-db-oracle sh -c 'chown -R oracle:oinstall /opt/oracle/oradata/termterm-tls; chmod 700 /opt/oracle/oradata/termterm-tls'
docker exec -u oracle termterm-db-oracle sh -c 'rm -f /opt/oracle/oradata/termterm-tls/cwallet.sso /opt/oracle/oradata/termterm-tls/ewallet.p12; orapki wallet create -wallet /opt/oracle/oradata/termterm-tls -pwd "$(cat /opt/oracle/oradata/termterm-tls/password)" -auto_login; orapki wallet add -wallet /opt/oracle/oradata/termterm-tls -pwd "$(cat /opt/oracle/oradata/termterm-tls/password)" -dn CN=localhost -keysize 2048; orapki wallet export -wallet /opt/oracle/oradata/termterm-tls -pwd "$(cat /opt/oracle/oradata/termterm-tls/password)" -dn CN=localhost -request /opt/oracle/oradata/termterm-tls/server.csr' >/dev/null
docker cp termterm-db-oracle:/opt/oracle/oradata/termterm-tls/server.csr "$tls/oracle.csr"
openssl x509 -req -in "$tls/oracle.csr" -CA "$tls/ca.pem" -CAkey "$tls/ca.key" -CAcreateserial -days 365 -sha256 -extfile "$tls/server.ext" -out "$tls/oracle.pem" 2>/dev/null
for file in ca.pem oracle.pem; do docker cp "$tls/$file" "termterm-db-oracle:/opt/oracle/oradata/termterm-tls/$file"; done
docker exec -u oracle termterm-db-oracle sh -c 'orapki wallet add -wallet /opt/oracle/oradata/termterm-tls -pwd "$(cat /opt/oracle/oradata/termterm-tls/password)" -trusted_cert -cert /opt/oracle/oradata/termterm-tls/ca.pem; orapki wallet add -wallet /opt/oracle/oradata/termterm-tls -pwd "$(cat /opt/oracle/oradata/termterm-tls/password)" -user_cert -cert /opt/oracle/oradata/termterm-tls/oracle.pem' >/dev/null
cat > "$tls/listener.ora" <<'EOF'
DEFAULT_SERVICE_LISTENER = FREE
LISTENER = (DESCRIPTION_LIST=(DESCRIPTION=(ADDRESS=(PROTOCOL=TCP)(HOST=0.0.0.0)(PORT=1521))(ADDRESS=(PROTOCOL=TCPS)(HOST=0.0.0.0)(PORT=1522))(ADDRESS=(PROTOCOL=IPC)(KEY=EXTPROC1521))))
WALLET_LOCATION=(SOURCE=(METHOD=FILE)(METHOD_DATA=(DIRECTORY=/opt/oracle/oradata/termterm-tls)))
SSL_CLIENT_AUTHENTICATION=FALSE
EOF
cat > "$tls/sqlnet.ora" <<'EOF'
NAMES.DIRECTORY_PATH=(TNSNAMES,EZCONNECT,HOSTNAME)
DISABLE_OOB=ON
SQLNET.EXPIRE_TIME=3
WALLET_LOCATION=(SOURCE=(METHOD=FILE)(METHOD_DATA=(DIRECTORY=/opt/oracle/oradata/termterm-tls)))
SSL_CLIENT_AUTHENTICATION=FALSE
EOF
for file in listener.ora sqlnet.ora; do docker cp "$tls/$file" "termterm-db-oracle:/opt/oracle/product/26ai/dbhomeFree/network/admin/$file"; done
docker exec -u oracle termterm-db-oracle lsnrctl stop >/dev/null || true
docker exec -u oracle termterm-db-oracle lsnrctl start >/dev/null
echo 'Disposable SQL Server TLS and Oracle TCPS configured. Oracle TCPS: container port 1522 (loopback relay required).'
