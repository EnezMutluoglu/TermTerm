export type VpnFamily = "openVpn" | "wireGuard" | "fortinet" | "globalProtect";
export type VpnProfile = {
  id: string;
  version: number;
  label: string;
  family: VpnFamily;
  source: string;
  server: string;
  groupId: string | null;
  username: string;
  hasPassword: boolean;
  hasConfig: boolean;
  realm: string;
  remember: boolean;
  pending?: boolean;
  revision?: number;
  visibility?: "private" | "shared";
  permissions?: string[];
  canRemove?: boolean;
  canUpdate?: boolean;
  scopeId?: string | null;
};
export type VpnChallenge = {
  id: number;
  kind?: "sso";
  provider?: "globalProtect";
  stage?: "portal" | "gateway";
  message?: string;
  error?: string;
  fields: {
    name: string;
    label: string;
    secret: boolean;
    purpose: string;
    value?: string;
    choices?: { value: string; label: string }[];
  }[];
};
export type VpnSession = {
  id: string;
  vaultId: string;
  profileId: string;
  generation: number;
  state:
    | "connecting"
    | "challenge"
    | "connected"
    | "disconnecting"
    | "disconnected"
    | "failed";
  error?: string;
  authOnly?: boolean;
  authStage?: "portal" | "gatewaySelection" | "gateway";
  authenticationVerified?: boolean;
  challenge?: VpnChallenge;
};
export const vpnActive = (session?: VpnSession) =>
  !!session && !["disconnected", "failed"].includes(session.state);

// Initial IPC snapshots cannot replace events already received for the same session.
export function mergeVpnSnapshot(all: VpnSession[], next: VpnSession): VpnSession[] {
  return all.some(session => session.id === next.id && session.generation === next.generation)
    ? all : mergeVpnSession(all,next);
}

// A finished generation cannot be revived by a late engine event.
export function mergeVpnSession(
  all: VpnSession[],
  next: VpnSession,
): VpnSession[] {
  const previous = all.find((session) => session.profileId === next.profileId);
  if (
    previous &&
    (previous.generation > next.generation ||
      (previous.generation === next.generation &&
        ((previous.state === "connected" && next.state === "connecting") ||
          (previous.challenge &&
            next.challenge &&
            previous.challenge.id > next.challenge.id) ||
          (!vpnActive(previous) && vpnActive(next)) ||
          (previous.state === "failed" && ["disconnected", "disconnecting"].includes(next.state)) ||
          (previous.state === "disconnecting" &&
            ["connecting", "challenge", "connected"].includes(next.state)))))
  )
    return all;
  return [
    ...all.filter((session) => session.profileId !== next.profileId),
    next,
  ];
}

export const vpnPermitted = (profile: VpnProfile, permission: string) => profile.visibility !== "shared" || !!profile.permissions?.includes(permission);
