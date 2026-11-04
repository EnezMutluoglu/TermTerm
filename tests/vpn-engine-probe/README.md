# Actual Windows engine packet probe

This opt-in local integration test runs unelevated, through the broker's real
suspended child/job/private handle path. It does not load Wintun or call Windows
adapter, address, route, or DNS APIs. It needs disposable loopback VPN servers
from `scripts/test-vpn-lab.py --desktop-fixture .lab/vpn-desktop` and the audited
Windows helpers with their DLLs. The fixture directory must have a private ACL;
profiles, credentials and keys must never enter source or logs.

```powershell
cargo run --locked --manifest-path tests/vpn-engine-probe/Cargo.toml -- `
  .lab/vpn-desktop/profiles.json .tools/vpn-windows/engine `
  artifacts/vpn-windows/engines-real-packet-20261022.json
```

The probe negotiates two OpenVPN connections and one AnyConnect-compatible
ocserv connection, checks raw IP packet integrity through the actual engines,
then stops the middle child and checks both survivors. Ocserv does not validate
Fortinet or GlobalProtect servers. Packet-pipe success does not establish that
the elevated adapter/network ownership path works.

Set `TERMTERM_VPN_PROBE_OPENVPN_ONLY=1` to validate all three OpenVPN loopback
fixtures (31194/31195/31198) before the separately reviewed elevated runtime test.
The report derives its protocol list from the selected fixtures.
