export type DatabaseEngine = "postgresql" | "mssql" | "oracle";
export interface DatabaseConnection {
  label: string;
  engine: DatabaseEngine;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  tls: "verify-full" | "disabled-local";
  caPath: string;
  caPem?: string;
  environment: "development" | "test" | "production";
  groupId: string;
  sshHostId: string;
  serverDn?: string;
  oracleRole?: "normal" | "sysdba";
  autoCommit?: boolean;
  healthWarning?: number;
  healthCritical?: number;
}
export interface RowSet {
  columns: { name: string; type: string }[];
  rows: (string | null)[][];
  truncated: boolean;
  affectedRows?: number;
}
export interface QueryResult {
  messages?: string[];
  operationId?: string;
  state?: string;
  status?: string;
  message?: string;
  observedAt?: number;
  historyWarning?: string;
  results: RowSet[];
  elapsedMs: number;
  transaction: boolean;
  autoCommit: boolean;
}
export interface DatabaseTab {
  libraryLabel?: string;
  id: string;
  recordId: string;
  title: string;
  engine: DatabaseEngine;
  sql: string;
  parameters?: string;
  busy: boolean;
  transaction: boolean;
  autoCommit: boolean;
  result?: QueryResult;
  error?: string;
  operationId?: string;
  state?: string;
  capabilities?: Record<string, unknown>;
  target: Pick<
    DatabaseConnection,
    "host" | "port" | "database" | "environment"
  >;
}
export const engineNames = {
  postgresql: "PostgreSQL",
  mssql: "SQL Server",
  oracle: "Oracle",
};
export const defaultConnection = (): DatabaseConnection => ({
  label: "",
  engine: "postgresql",
  host: "localhost",
  port: 5432,
  database: "postgres",
  username: "",
  password: "",
  tls: "verify-full",
  caPath: "",
  environment: "development",
  groupId: "",
  sshHostId: "",
  autoCommit: true,
});
