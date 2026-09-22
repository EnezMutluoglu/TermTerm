export type EntityKind =
  | "host"
  | "group"
  | "credential"
  | "snippet"
  | "workspace"
  | "tunnel"
  | "knownHost"
  | "log"
  | "settings"
  | "syncProfile"
  | "integration";
export interface Entity {
  id: string;
  kind: EntityKind;
  data: Record<string, any>;
  updatedAt: number;
}
export interface Vault {
  id: string;
  name: string;
  path: string;
  deviceId: string;
  records: Entity[];
}
export interface AppInfo {
  defaultVaultPath: string;
  home: string;
  recent?: { path: string };
  version: string;
  platform?: {
    os: string;
    arch: string;
    defaultShell: string;
    agentKinds: string[];
    rememberStore: string;
  };
}
export interface Prompt {
  id: string;
  sessionId: string;
  kind: string;
  detail: {
    address?: string;
    fingerprint?: string;
    algorithm?: string;
    name?: string;
    instructions?: string;
    prompts?: { prompt: string; echo: boolean }[];
  };
}
export interface Session {
  id: string;
  label: string;
  hostId?: string;
  status: string;
  connected: boolean;
  closed: boolean;
  statsEnabled?: boolean;
}
export interface FileEntry {
  name: string;
  path: string;
  directory: boolean;
  symlink: boolean;
  size: number;
  modified: number;
  permissions?: number;
}
export interface Endpoint {
  connection: string;
  path: string;
}
export interface ImportPreview {
  format: string;
  records: Entity[];
  warnings: string[];
  items?: {
    recordId: string | null;
    label: string;
    status: string;
    notes: string[];
  }[];
}
export interface ImportApplyResult {
  vault: Vault;
  added: number;
  updated: number;
  skipped: number;
  failed: number;
  items: { sourceId: string; targetId: string; action: string }[];
}
export interface SyncProfile {
  label: string;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  schema: string;
  caPath: string;
  tls: string;
}
export const newEntity = (
  kind: EntityKind,
  data: Entity["data"] = {},
): Entity => ({ id: crypto.randomUUID(), kind, data, updatedAt: Date.now() });
export const defaultProfile: SyncProfile = {
  label: "PostgreSQL",
  host: "localhost",
  port: 55432,
  database: "termterm_dev",
  username: "termterm_app",
  password: "",
  schema: "termterm",
  caPath: "",
  tls: "verify-full",
};
