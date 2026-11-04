# Windows VPN development broker

The isolated Windows broker passed a real three-tunnel lab test on 2026-10-22: two OpenVPN sessions and one Cisco/OpenConnect session authenticated, carried Windows kernel traffic, survived stopping one session, and reconnected it with a new generation. All 24 ICMP payload checks passed. Authenticated cleanup reported zero sessions and protected journals; independent native GUID/PnP checks were empty and addresses, routes, interface settings and DNS matched the saved host baseline.

This is development evidence. Normal desktop `readiness()` and `connect()` remain disabled. No installed TermTerm files, stable vault, installer, service, release tag or update feed were changed. Exact evidence paths, hashes and resume boundaries are in [HANDOFF.md](HANDOFF.md).

## Runtime boundaries

The broker retains the real manager process and verifies its PID, creation time, user SID, logon SID/authentication ID and Windows session. Only the elevated broker boundary accepts the operating system's exact Full-to-linked-Limited token relationship. Ordinary client impersonation, engine and watchdog checks retain strict identity matching. An elevated manager, alternate administrator or unrelated logon is rejected. Pipes deny remote clients, have an explicit same-logon ACL and use exclusive first instances; impersonation always reverts.

Engines inherit the original unelevated manager token through `CreateProcessW` and `PROC_THREAD_ATTRIBUTE_PARENT_PROCESS`. The retained authenticated manager is the designated parent. Only control input/output/error and the raw packet endpoint are duplicated into that parent's handle table and listed for inheritance. Remote copies close on success and failure. A suspended child must join its kill-on-close job and pass actual unelevated-token verification before resuming. No process-creation privilege is granted and there is no elevated-engine fallback.

Each packet pipe message is one complete raw IPv4/IPv6 packet, with no length prefix. Packet length, IP header lengths and negotiated MTU are checked. The inherited endpoint is not registered with the broker's IOCP. Wintun adapters, exact on-link routes, assigned addresses and MTU belong to the broker; engines never open a host adapter or execute imported scripts. The packet pump starts before actual address readiness is polled. Both engine-connected and Windows `DadState=Preferred` are required for UI connected; tentative addresses have a ten-second limit. Cancellation or failed cleanup keeps that generation terminal.

At most three sessions, including pending reservations and failed cleanup, are admitted. Negotiated route/address prefixes are checked against all broker reservations and existing nondefault host/LAN/other-VPN routes and address prefixes. Gateways are retained as proposal metadata; Wintun transports layer-3 packets and routes use on-link semantics.

Durable protected intent precedes every mutation. Reverse cleanup verifies the exact GUID/LUID and managed value and retains reservations on failure. The separate watchdog holds the machine reservation until recovery finishes. An owned phantom can be removed only through exact protected-journal, name, GUID/LUID, driver and native nonpresent-device proofs, using single-device SetupAPI removal. Normal close can use this path only after this broker successfully closed its own raw adapter and all address/route/MTU undo finished. Live, changed or unknown devices fail closed. Recovery loads no Wintun DLL and never opens an existing adapter.

## Outstanding production scope

The proven lab uses IPv4 split routes, MTU, OpenVPN and Cisco/ocserv with ordinary CA verification. OpenConnect dotted IPv4 route masks are normalized after validating contiguity. Empty semantic DNS metadata is accepted; nonempty DNS servers/domains/search policy, proxy/PAC, excluded routes, full tunnels, net30 and OpenConnect IPv6 address plans remain unsupported. IPv6 framing and policy code do not establish an IPv6 kernel test.

Fortinet and GlobalProtect families are wired, but real vendor-server authentication, enterprise MFA/SSO/hardware keys and production-server compatibility have not been tested. A crash after adapter creation but before any durable LUID-bearing address/MTU/route entry can require retained-journal review; ownership is not invented.

The pinned official Wintun 0.14.1 DLL has global orphan/legacy cleanup behavior. Read-only native inventory guards run before Load/Create/Close. Its driver-install path can still upgrade older drivers or affect other Wintun users; the successful scoped test used the exact installed driver with no unrelated Wintun devices. General upgrade/coexistence remains a production blocker. The signed driver package remains installed, with its service stopped; no global `WintunDeleteDriver` was attempted.

## Checks and explicit validation

Final independent checks passed: default suite **44/44**, diagnostic-feature suite **46/46**, and full-runner self-tests **11/11**. These suites do not load real Wintun, prompt for UAC, create adapters or alter host networking.

```powershell
& "$env:USERPROFILE/.cargo/bin/cargo.exe" test --manifest-path vpn-windows-broker/Cargo.toml
& "$env:USERPROFILE/.cargo/bin/cargo.exe" test --manifest-path vpn-windows-broker/Cargo.toml --features privileged-validation
```

The default binary rejects privileged serving/watchdog and validation commands before elevation, protected-state access or DLL loading. `--wintun-sweep-preflight` is read-only native inventory. The `privileged-validation` feature permits explicit diagnostics; it does not enable normal readiness/connect/serve.

The reviewed owned-object runner is `scripts/validate-privileged-v3.ps1`: three fresh adapters, TEST-NET addresses `192.0.2.241` through `.243/32`, on-link routes `198.51.100.241` through `.243/32`, and MTU 1400. Broker-death exit 92 alone is never a pass: external watchdog, native identity, journal and persisted-baseline proof are required. The reviewed real-engine runner is `scripts/validate-full-broker-v5.ps1`, restricted by the helper to disposable loopback fixtures. Neither runner is an unattended build/test step.

Preserve every frozen bundle and failure report. Future privileged runs need a newly reviewed bundle/manifest, current fixture CA/profile hashes and an explicit scope. Historical credentials/certificates must not be reused after the disposable fixture stops. Do not stage a diagnostic-feature helper as normal desktop support or treat lab evidence as release approval.
