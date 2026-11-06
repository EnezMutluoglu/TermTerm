import { describe, expect, it } from "vitest";
import { mergeVpnSession, mergeVpnSnapshot, vpnActive, type VpnSession } from "../src/vpnState";
const session = (
  profileId: string,
  generation: number,
  state: VpnSession["state"],
): VpnSession => ({
  id: `${profileId}-${generation}`,
  vaultId: "test",
  profileId,
  generation,
  state,
});
describe("VPN connection event ordering", () => {
  it("keeps Windows cleanup pending active until the original failure is delivered", () => {
    const pending = {...session("a",1,"disconnecting"),error:"cleanup_pending: windows_interface_unregister_pending"};
    expect(vpnActive(pending)).toBe(true);
    expect(mergeVpnSession([pending],session("a",1,"connected"))[0]).toBe(pending);
    const failure = {...session("a",1,"failed"),error:"engine_failed: wireguard_peer_unresponsive"};
    expect(mergeVpnSession([pending],failure)[0]).toBe(failure);
    expect(vpnActive(failure)).toBe(false);
  });
  it("keeps a failure through same-generation cleanup and clears it on an explicit new attempt", () => {
    const failure = {...session("a",1,"failed"),error:"Gateway SSO giriş testi tamamlanamadı; tünel açılmadı.",authOnly:true};
    for (const state of ["disconnecting", "disconnected"] as const) {
      expect(mergeVpnSession([failure],session("a",1,state))[0]).toBe(failure);
    }
    const retry = mergeVpnSession([failure],session("a",2,"connecting"))[0];
    expect(retry.state).toBe("connecting");
    expect(retry.error).toBeUndefined();
  });
  it("keeps an early challenge when connect or auth-test replies arrive later, while answer events can resume connecting",()=>{
    const challenge = {...session("a",1,"challenge"),challenge:{id:1,kind:"sso" as const,fields:[]}};
    for(const command of ["vpn_connect","vpn_sso_test_start"]) {
      const events=[challenge];
      expect(mergeVpnSnapshot(events,session("a",1,"connecting")),command).toBe(events);
    }
    expect(mergeVpnSession([challenge],session("a",1,"connecting"))[0].state).toBe("connecting");
  });
  it("keeps three independent sessions and rejects stale generations", () => {
    const all = [
      session("a", 4, "connected"),
      session("b", 2, "connected"),
      session("c", 3, "challenge"),
    ];
    expect(mergeVpnSession(all, session("a", 1, "failed"))).toBe(all);
    expect(mergeVpnSession(all, session("a", 4, "connecting"))).toBe(all);
    const closed = mergeVpnSession(all, session("b", 2, "disconnected"));
    expect(closed.filter(vpnActive).map((item) => item.profileId)).toEqual([
      "a",
      "c",
    ]);
    expect(mergeVpnSession(closed, session("b", 2, "connected"))).toBe(closed);
    expect(
      mergeVpnSession(closed, session("b", 5, "connecting")).filter(vpnActive),
    ).toHaveLength(3);
  });
  it("does not revive a disconnecting generation with a late challenge", () => {
    const all = [session("a", 1, "disconnecting")];
    expect(mergeVpnSession(all, session("a", 1, "challenge"))).toBe(all);
    expect(mergeVpnSession(all, session("a", 1, "failed"))[0].state).toBe(
      "failed",
    );
  });
});
