#!/usr/bin/env python3
"""Explicit operator tooling. No server-side bootstrap or automatic migration.

Uses libpq environment + a private PGPASSFILE, never command-line passwords.
All database traffic verifies the operator-supplied CA and hostname.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def literal(value):
    return "'" + value.replace("'", "''") + "'"


def environment():
    env = os.environ.copy()
    for name in ["PGHOST", "PGDATABASE", "PGUSER", "PGSSLROOTCERT", "PGPASSFILE"]:
        if not env.get(name):
            raise ValueError("Set " + name)
    path = Path(env["PGPASSFILE"])
    if not path.is_file() or stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise ValueError("PGPASSFILE must exist with mode 0600 or stricter")
    if not Path(env["PGSSLROOTCERT"]).is_file():
        raise ValueError("PostgreSQL CA file missing")
    env["PGSSLMODE"] = "verify-full"
    env["PGCONNECT_TIMEOUT"] = "5"
    env.pop("PGPASSWORD", None)
    return env


def run(sql):
    p = subprocess.run(["psql", "-X", "-qAt", "-v", "ON_ERROR_STOP=1"],
                       input=sql, text=True, capture_output=True, env=environment())
    if p.returncode:
        # SQL can include bootstrap names; never echo arbitrary server details.
        raise RuntimeError("Database operation failed; check connectivity, migration privileges and schema compatibility")
    return p.stdout.strip()


def private_write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        json.dump(value, f, ensure_ascii=False, indent=2)
        f.write("\n")


def migration_files():
    files = sorted((ROOT / "migrations").glob("[0-9]*.sql"))
    versions = [int(p.name.split("_")[0]) for p in files]
    if not versions or versions != list(range(1, versions[-1] + 1)):
        raise ValueError("Migration sequence is incomplete")
    return files


HASH_TABLE = """CREATE TABLE IF NOT EXISTS termterm_team_v2.migration_file_hashes (
 version integer PRIMARY KEY REFERENCES termterm_team_v2.schema_migrations(version),
 sha256 text NOT NULL CHECK(sha256 ~ '^[0-9a-f]{64}$'),
 verified_at timestamptz NOT NULL DEFAULT clock_timestamp());
 ALTER TABLE termterm_team_v2.migration_file_hashes ENABLE ROW LEVEL SECURITY;
 REVOKE ALL ON termterm_team_v2.migration_file_hashes FROM PUBLIC;"""


def migrate(role, execute=None):
    if not re.fullmatch(r"[a-z_][a-z0-9_]{0,62}", role):
        raise ValueError("Service role must be a lowercase PostgreSQL identifier")
    files = migration_files()
    if execute is None and run("SELECT to_regclass('termterm_team_v2.schema_migrations') IS NOT NULL") == 't':
        installed = int(run("SELECT coalesce(max(version),0) FROM termterm_team_v2.schema_migrations"))
        if installed < len(files):
            raise ValueError("Existing database needs an upgrade: use upgrade.py plan/apply with a verified backup")
    statements = ["SET statement_timeout='5min'; SET lock_timeout='15s';",
                  "SELECT pg_advisory_lock(hashtextextended(current_database() || ':termterm-team-migration',0));",
                  "SELECT to_regclass('termterm_team_v2.schema_migrations') IS NOT NULL AS exists \\gset",
                  "\\if :exists",
                  f"DO $$ BEGIN IF (SELECT max(version) FROM termterm_team_v2.schema_migrations)>{len(files)} THEN RAISE EXCEPTION 'Database schema newer than package'; END IF; END $$;",
                  "\\endif"]
    # Verify ALL existing checksums before executing any new SQL or grants.
    statements += ["SELECT to_regclass('termterm_team_v2.migration_file_hashes') IS NOT NULL AS hashes \\gset", "\\if :hashes"]
    for path in files:
        version=int(path.name.split('_')[0]); digest=hashlib.sha256(path.read_text(encoding='utf-8').encode()).hexdigest()
        statements.append(f"DO $$ BEGIN IF EXISTS(SELECT FROM termterm_team_v2.migration_file_hashes WHERE version={version} AND sha256<>'{digest}') THEN RAISE EXCEPTION 'Applied migration checksum mismatch'; END IF; END $$;")
    statements += ["\\endif"]
    for version, path in enumerate(files, 1):
        content=path.read_text(encoding='utf-8')
        if not content.rstrip().endswith('COMMIT;'):
            raise ValueError('Each migration must end in its own COMMIT')
        digest=hashlib.sha256(content.encode()).hexdigest()
        record=HASH_TABLE+f"\nINSERT INTO termterm_team_v2.migration_file_hashes(version,sha256) VALUES({version},'{digest}') ON CONFLICT DO NOTHING;"
        # The schema revision and its file identity commit atomically. A killed
        # tool can resume without adopting an unverified half-applied migration.
        atomic=content.rstrip()[:-len('COMMIT;')]+record+'\nCOMMIT;'
        statements += ["SELECT to_regclass('termterm_team_v2.schema_migrations') IS NOT NULL AS exists \\gset",
                       "\\if :exists",f"SELECT EXISTS(SELECT FROM termterm_team_v2.schema_migrations WHERE version={version}) AS applied \\gset",
                       "\\else","\\set applied false","\\endif","\\if :applied",record,"\\else",atomic,"\\endif"]
    statements.append((ROOT/'migrations/grant_service.sql').read_text(encoding='utf-8').replace(':"service_role"','"'+role+'"'))
    (execute or run)("\n".join(statements))
    print(f"Verified {len(files)} migrations and explicit grants. No account was bootstrapped.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    m = commands.add_parser("migrate")
    m.add_argument("--service-role", required=True)
    b = commands.add_parser("bootstrap")
    b.add_argument("--team", required=True)
    b.add_argument("--username", required=True)
    b.add_argument("--display-name", required=True)
    b.add_argument("--output", required=True, help="New private file for one-time credentials")
    a = parser.parse_args()
    if a.command == "migrate":
        migrate(a.service_role)
    else:
        # Reserve the output before creating the account. Never overwrite an
        # earlier bootstrap or silently create another team on a file error.
        path = Path(a.output)
        path.parent.mkdir(parents=True, exist_ok=True)
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            result = run("SELECT termterm_team_v2.bootstrap_team(" + ",".join(map(literal,
                         [a.team, a.username, a.display_name])) + ");")
            data = json.loads(result)
            json.dump(data, f, indent=2)
            f.write("\n"); f.flush(); os.fsync(f.fileno())
        print("Initial leader created. One-time credentials saved in the private output file; no credentials printed.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, RuntimeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
