#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .lab/database
umask 077
if [ ! -f .lab/database/credentials.env ]; then
  printf 'MSSQL_SA_PASSWORD=Tt!%s9a\nORACLE_PWD=Tt%s9A\nPG_PASSWORD=Tt%s9A\n' "$(openssl rand -hex 20)" "$(openssl rand -hex 20)" "$(openssl rand -hex 20)" > .lab/database/credentials.env
fi
set -a; source .lab/database/credentials.env; set +a
action=${1:-status}
if [ "$action" = 'up' ]; then
  for spec in 'mssql mcr.microsoft.com/mssql/server@sha256:4402d880dd4c34bfa7d8705e56a86cd6c88da80a1f6bbbe741f999e76264a090 41433 1433 6g' 'oracle container-registry.oracle.com/database/free@sha256:f988b0c04c4c386cd306a2a914c0d7a9702d83acc31b064a28ad8eb6278a8fba 41521 1521 4g'; do
    read -r name image hostport port memory <<< "$spec"
    if ! docker container inspect "termterm-db-$name" >/dev/null 2>&1; then
      docker pull "$image"
      pinned=$(docker image inspect "$image" --format '{{index .RepoDigests 0}}')
      printf '%s\n' "$pinned" > ".lab/database/$name.image"
      if [ "$name" = mssql ]; then
        docker run -d --name termterm-db-mssql --label app=termterm-db-lab --memory "$memory" --cpus 2 --restart=no -p "127.0.0.1:$hostport:$port" -e ACCEPT_EULA=Y -e MSSQL_PID=Developer -e MSSQL_SA_PASSWORD -v termterm-db-mssql:/var/opt/mssql "$pinned"
      else
        docker run -d --name termterm-db-oracle --label app=termterm-db-lab --memory "$memory" --cpus 2 --restart=no -p "127.0.0.1:$hostport:$port" -e ORACLE_PWD -v termterm-db-oracle:/opt/oracle/oradata "$pinned"
      fi
    else docker start "termterm-db-$name"; fi
  done
  pg_ctlcluster 18 termterm start || test "$(pg_lsclusters -h | awk '$2=="termterm" {print $4}')" = online
  if ! runuser -u postgres -- psql -p 55432 -Atqc "SELECT 1 FROM pg_roles WHERE rolname='termterm_sql_lab'" | grep -q 1; then
    printf "CREATE ROLE termterm_sql_lab LOGIN PASSWORD '%s';\n" "$PG_PASSWORD" | runuser -u postgres -- psql -p 55432 -v ON_ERROR_STOP=1 >/dev/null
    runuser -u postgres -- createdb -p 55432 -O termterm_sql_lab termterm_sql_lab
  fi
elif [ "$action" = stop ]; then
  docker stop termterm-db-mssql termterm-db-oracle
elif [ "$action" != status ]; then
  echo 'Usage: db-lab.sh up|stop|status' >&2; exit 2
fi
docker ps -a --filter label=app=termterm-db-lab --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'
