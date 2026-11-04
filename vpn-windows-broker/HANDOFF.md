# Windows broker handoff — 2026-10-22

Worktree: `<VPN_WORKTREE>`, branch `codex/feature-vpn-workspace`. Runtime and frozen bundles remain unchanged after full-v9 passed. This note contains no credentials or private profile contents. Parent alone executed UAC; this worker changed source, built and ran unelevated checks.

## Successful evidence

**Full Windows kernel path: PASS.** Local ignored report:

`.tools/vpn-windows/broker-full-validation-v9/full-broker-result.json`

SHA-256: `4dcea13314856c7f15c76066e9bd8fc83ba8c38af5f31b544be6ced6b1d27907`.

Two OpenVPN engines and one Cisco/OpenConnect engine authenticated against disposable loopback OpenVPN/ocserv servers, using ordinary CA verification for ocserv. All 24 Windows ICMP payload checks passed: nine with all three connected, six through surviving sessions after the middle session stopped, and nine after reconnection. The same UUID reconnected at generation 2; a generation-1 disconnect was rejected. Authenticated Close returned zero sessions and protected journals. Native GUID/PnP inventories were empty before/after, no broker/engine processes or test adapters remained, and addresses/routes/interfaces/DNS matched the persisted baseline. An independent verifier checked the exact report, all bundle members and current host state.

Frozen bundle: `.tools/vpn-windows/broker-full-validation-v9` (30 manifest members).

| Artifact | SHA-256 |
| --- | --- |
| `termterm-vpn-broker.exe` | `b80f00077441af24c6e57bd268d217627b59d615656acf8b277aa4fe31ee41fe` |
| `manifest.json` | `aaaf31d063f593cf9588bb5902e036bd1435f19d828cc6254089e9cf03db812c` |
| `scripts/validate-full-broker-v5.ps1` | `4bed56f81c0173684a6238ba59a96c4e9524e58ebc9ab3fc47366697d7459236` |
| `termterm-openvpn.exe` | `a1258dfdadc5ae5169b46476f32e0908e089176a98817b7f523b9abf40b6d5b5` |
| `termterm-openconnect.exe` | `eb724cf8a36cc9cb288f1a68f96656a9380eca0ef2e3fac05de5f03c3f1f91f2` |
| Patched OpenConnect 9.21 DLL | `59d60129ea53ca903448a17d61a25034a38849ceb64c13d2733be1b647f363bf` |
| Official Wintun 0.14.1 x64 DLL | `e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce` |
| Disposable public fixture CA | `42366ce749f37626f0535b349d851789027097ca9a89ceaf3268d0035d4c8475` |
| Private fixture-file hash only | `4423287099f65d68aa0aebf11028930afa5cb3869c356d7c3bcd0986b06e4ea6` |

**Recovery, normal objects and broker-death watchdog: PASS.** Earlier frozen v6 report:

`.tools/vpn-windows/recovery-v3-result-20261022T152447808-b08bc3a749214f0d909836ea2816b441/recovery-result.json`

Report SHA-256: `a7d6acdb55e843785dd40738a563ae1c6caf30eb9095c0758ed080098dbd9e0d`. This sequence removed three exact owned phantom devices/journals, passed three normal objects and passed broker-death cleanup. The external witness proved empty journals, native GUID/devnode absence and baseline equality. Its earlier helper was `bcb42801aa917b36956d969ab00c86ae00d9c17a7f4e42e8f0c711f4f15dff90`; full-v9 separately exercised normal cleanup through the current runtime.

**Elevated-to-unelevated launch primitive: PASS.** Report:

`.tools/vpn-windows/engine-launch-v1-result-20261022T155644909-594fbd6227ef43b28b802d901a6adb02/engine-launch-result.json`

Report SHA-256: `681192139d77edd256cfed134ff65ca0493aff090f8aa7a2bb083009251f5e65`. Original manager token, explicit inherited control/raw packet handles, excluded original event, suspended job assignment, packet echo and kill-on-job-close all passed without host-network APIs. This earlier helper was `d8cf5d3b15f9edd3d621524c21df1c3c587691a4174150f86d9701de04067b2a`; the same shared launcher was then used by full-v9.

Final independent checks: default **44/44**, `privileged-validation` **46/46**, runner-v5 self-tests **11/11**. Final diagnostic helper compiled in release profile. After the final runtime freeze, only this document and README were changed; no reruns or builds followed.

## Implementation decisions to retain

- Ordinary identity checks stay strict. Only actual elevated-broker boundaries accept an OS-provided Full-to-linked-Limited pair matching the retained owner; matching user/session metadata alone never suffices.
- Actual UAC `CreateProcessAsUserW` failed with error 1314. The engine launcher now uses the authenticated original manager as `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS`, four explicit remote handle copies, original-owner environment, suspended job/token verification and RAII copy cleanup. No new privileges, credentials, Explorer parent or elevated engine fallback were introduced.
- One packet is one message. The engine endpoint never joins the broker IOCP before inheritance. Oversize, EOF and cancellation tests use an actual child.
- Normal-close phantom completion requires the complete durable journal, a consistent recorded LUID, successful creator raw Close, removed map ownership and a sole remaining adapter undo. Shared strict native phantom checks and exact SetupAPI removal follow; other sessions are untouched.
- Recovery validates all targets before removal, rejects live/unknown/reused identities and restart requirements, and requires both raw GUID and devnode absence. Recovery loads no Wintun DLL.
- Windows address creation is asynchronous. The pump runs before regular-tick DAD polling; Preferred plus engine-connected is required for UI connected. Tentative waits at most ten seconds. Cancellation, DAD failure/timeout or failed cleanup leaves a sticky terminal generation, even with retained tun/reservation and queued engine messages.

## Outstanding production boundaries

`PRIVILEGED_PATH_VALIDATED` remains false. Normal desktop readiness/connect/serve are disabled, including diagnostic-feature builds. A default helper must be built/staged separately; diagnostic helpers must not be shipped as normal desktop support.

Actual kernel evidence covers IPv4 split-route OpenVPN and Cisco/ocserv only. Real Fortinet/GlobalProtect vendor authentication, enterprise MFA/SSO/hardware keys, production servers, enterprise DNS and full-tunnel/coexistence policy remain untested. Nonempty negotiated DNS/domains/search policy, proxy/PAC, exclusions, net30 and OpenConnect IPv6 address plans are rejected. IPv6 framing/policy is not an IPv6 kernel test.

Official Wintun global orphan/legacy sweeps are guarded by native read-only inventory before Load/Create/Close. Its driver-install/upgrade path can still affect other Wintun users. The scoped test used the installed exact version and no unrelated Wintun devices; general coexistence/upgrade is unresolved. The signed driver package remains installed with its service stopped. Global driver deletion was not attempted.

An adapter-only crash intent with no recorded LUID is deliberately retained if an orphan exists. Do not claim every adapter-creation crash window is covered or infer missing ownership from a friendly name.

## Resume boundaries

Preserve every frozen bundle and failure report, including earlier normal/crash failures and full-v4 through v8. Full-v8 was staged but never run. The disposable WSL fixture has now stopped with its 35/35 cleanup checks verified; central status/resume documents are owned by the other worker. This manifest is historical evidence, not authorization to reuse stopped fixture credentials/certificates.

A future fixture restart requires fresh private profiles and CA, a new bundle directory/manifest and reviewed hashes. Never commit private profiles, passwords, local lab state, caches or secret-bearing output. Keep normal readiness closed until driver/coexistence and remaining policy/crash boundaries have separately reviewed implementations and evidence. No installed stable TermTerm change, release, main merge, tag or update publication is authorized by this handoff.
