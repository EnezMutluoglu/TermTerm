#!/usr/bin/env bash
# Run the compiled native lib-test binary only in disposable network namespaces.
set -euo pipefail
if [[ ${1:-} != --inside ]]; then
    binary=$(realpath "${1:?Usage: test-vpn-bound-sockets.sh /path/to/termterm_lib-test-binary}")
    exec unshare --net -- bash "$0" --inside "$binary"
fi
binary=$2
[[ $(readlink /proc/self/ns/net) != $(readlink /proc/1/ns/net) ]]
work=$(mktemp -d)
peer=
capture=
cleanup() {
    [[ -z $capture ]] || { kill -INT "$capture" 2>/dev/null || true; wait "$capture" 2>/dev/null || true; }
    [[ -z $peer ]] || { kill "$peer" 2>/dev/null || true; wait "$peer" 2>/dev/null || true; }
    rm -f -- "$work/physical.pcap" "$work/capture.log"
    rmdir "$work"
}
trap cleanup EXIT
unshare --net -- sleep 120 &
peer=$!
for _ in {1..100}; do
    [[ $(readlink "/proc/$peer/ns/net") != $(readlink /proc/self/ns/net) ]] && break
    sleep .02
done
[[ $(readlink "/proc/$peer/ns/net") != $(readlink /proc/self/ns/net) ]]
ip link set lo up
ip link add tt-out0 type veth peer name tt-peer0
ip link set tt-peer0 netns "$peer"
ip address add 198.18.0.1/24 dev tt-out0
ip link set tt-out0 up
ip link add tt-vpn0 type dummy
ip address add 198.19.0.1/24 dev tt-vpn0
ip link set tt-vpn0 up
nsenter -t "$peer" -n ip link set lo up
nsenter -t "$peer" -n ip address add 198.18.0.2/24 dev tt-peer0
nsenter -t "$peer" -n ip link set tt-peer0 up
nsenter -t "$peer" -n python3 -c 'import socket,threading
def echo(c):
    with c:
        while data:=c.recv(4096): c.sendall(data)
s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);s.bind(("198.18.0.2",28173));s.listen()
while True: threading.Thread(target=echo,args=(s.accept()[0],),daemon=True).start()' &
server=$!
trap 'kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true; cleanup' EXIT
export TERMTERM_VPN_SOCKET_NAMESPACE_TEST=1
# A fresh socket can bind to a device on current kernels without NET_RAW/NET_ADMIN.
sleep .2
setpriv --bounding-set=-net_raw,-net_admin "$binary" vpn_lifecycle::tests::linux_namespace_bound_interface_connects --exact --ignored --nocapture
tcpdump -U -ni tt-out0 -w "$work/physical.pcap" 'tcp and dst host 198.18.0.2 and dst port 28173' >"$work/capture.log" 2>&1 &
capture=$!
sleep .2
setpriv --bounding-set=-net_raw,-net_admin "$binary" vpn_lifecycle::tests::linux_namespace_wrong_interface_has_no_fallback --exact --ignored --nocapture
kill -INT "$capture"
wait "$capture"
capture=
packets=$(tcpdump -nr "$work/physical.pcap" 2>/dev/null | wc -l)
[[ $packets -eq 0 ]] || { echo "FAIL: $packets target TCP packets escaped through physical interface" >&2; exit 1; }
echo 'PASS: pinned TCP succeeds without network capabilities; wrong-interface dial emitted zero physical target TCP packets.'
