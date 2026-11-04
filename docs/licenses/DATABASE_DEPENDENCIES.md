# Database Workspace dependencies

Versions used by the 0.5.0-dev.1 development workspace. Downloaded binaries are not tracked in Git.

| Component | Version | Published license |
|---|---|---|
| PostgreSQL JDBC | 42.7.8 | [BSD-2-Clause](https://jdbc.postgresql.org/about/license.html) |
| Microsoft JDBC | 13.2.1.jre11 | [MIT](https://github.com/microsoft/mssql-jdbc/blob/main/LICENSE) |
| Oracle JDBC Thin / ojdbc11 | 23.9.0.25.07 | [Oracle Free Use Terms and Conditions](https://www.oracle.com/downloads/licenses/oracle-free-license.html) |
| Gson | 2.13.2 | [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0.txt) |
| Eclipse Temurin runtime | 21 LTS | GPLv2 with Classpath Exception; legal notices included in the runtime |
| CodeMirror 6 / Lezer | pnpm-lock.yaml | MIT; upstream package notices retained |

JAR files retain their original embedded license/notice files. Runtime packages retain the `legal` directory. Dependency SHA-256 values and download URLs are included in the generated `database-helper/lib/checksums.json` and development package. Oracle Instant Client is not bundled.

The SQL Server Developer and Oracle Free database server images are development laboratory dependencies, not part of the application package. Their editions do not establish support or entitlement for production editions, Oracle 19c/RAC, or licensed diagnostic packs.
