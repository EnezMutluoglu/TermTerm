package main

import (
	"bufio"
	"bytes"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"golang.org/x/crypto/curve25519"
	"golang.zx2c4.com/wireguard/conn"
	"golang.zx2c4.com/wireguard/device"
	"io"
	"net"
	"os"
	"strings"
	"sync"
	"testing"
	"time"
)

func bufioReader(value string) *bufio.Reader { return bufio.NewReader(strings.NewReader(value)) }

type packetMemory struct {
	incoming, outgoing chan []byte
	closed             chan struct{}
	once               sync.Once
}

func memoryPacket() *packetMemory {
	return &packetMemory{incoming: make(chan []byte, 4), outgoing: make(chan []byte, 4), closed: make(chan struct{})}
}
func (p *packetMemory) Read(b []byte) (int, error) {
	select {
	case value := <-p.incoming:
		return copy(b, value), nil
	case <-p.closed:
		return 0, os.ErrClosed
	}
}
func (p *packetMemory) Write(b []byte) (int, error) {
	select {
	case p.outgoing <- bytes.Clone(b):
		return len(b), nil
	case <-p.closed:
		return 0, os.ErrClosed
	}
}
func (p *packetMemory) Close() error { p.once.Do(func() { close(p.closed) }); return nil }

func TestRunFailureReportsBeforeActualDevicePacketClose(t *testing.T) {
	for _, reportError := range []error{nil, io.ErrClosedPipe} {
		packet := memoryPacket()
		tun := newPacketDevice(packet, 1420)
		engine := device.NewDevice(tun, conn.NewDefaultBind(), &device.Logger{Verbosef: func(string, ...any) {}, Errorf: func(string, ...any) {}})
		reported := false
		finishRun(errors.New("wireguard_peer_handshake_timeout"), func(kind string, detail any) error {
			select {
			case <-packet.closed:
				t.Fatal("packet closed before the terminal failure was reported")
			default:
			}
			if kind != "failed" || detail != "wireguard_peer_handshake_timeout" {
				t.Fatal("terminal failure was changed")
			}
			reported = true
			return reportError
		}, func() { engine.Close(); tun.Close() })
		if !reported {
			t.Fatal("missing terminal failure")
		}
		select {
		case <-packet.closed:
		default:
			t.Fatal("device cleanup did not close the packet endpoint")
		}
	}
	closed := false
	finishRun(nil, func(string, any) error { t.Fatal("normal return reported a failure"); return nil }, func() { closed = true })
	if !closed {
		t.Fatal("normal return skipped cleanup")
	}
}

type testBind struct {
	conn.Bind
	port uint16
}

func (b *testBind) Open(port uint16) ([]conn.ReceiveFunc, uint16, error) {
	fns, actual, err := b.Bind.Open(port)
	b.port = actual
	return fns, actual, err
}
func keys(t *testing.T) (string, string) {
	t.Helper()
	private := make([]byte, 32)
	if _, err := rand.Read(private); err != nil {
		t.Fatal(err)
	}
	public, err := curve25519.X25519(private, curve25519.Basepoint)
	if err != nil {
		t.Fatal(err)
	}
	return hex.EncodeToString(private), hex.EncodeToString(public)
}
func freshEngine(t *testing.T) (*device.Device, *testBind, *packetMemory, string) {
	t.Helper()
	private, public := keys(t)
	packet := memoryPacket()
	bind := &testBind{Bind: conn.NewDefaultBind()}
	engine := device.NewDevice(newPacketDevice(packet, 1420), bind, &device.Logger{Verbosef: func(string, ...any) {}, Errorf: func(string, ...any) {}})
	t.Cleanup(engine.Close)
	if err := engine.IpcSet("private_key=" + private + "\nlisten_port=0\n"); err != nil {
		t.Fatal(err)
	}
	if err := engine.Up(); err != nil {
		t.Fatal(err)
	}
	return engine, bind, packet, public
}
func addPeer(t *testing.T, engine *device.Device, public string, port uint16, route string) {
	t.Helper()
	if err := engine.IpcSet(fmt.Sprintf("public_key=%s\nendpoint=127.0.0.1:%d\nallowed_ip=%s\n", public, port, route)); err != nil {
		t.Fatal(err)
	}
}
func ready(engine *device.Device) bool {
	status := handshakeStatus{}
	return engine.IpcGetOperation(&status) == nil && status.peers > 0 && status.ready == status.peers
}
func TestRealHandshakeAndPayloadSHA(t *testing.T) {
	client, clientBind, clientPacket, clientPublic := freshEngine(t)
	server, serverBind, serverPacket, serverPublic := freshEngine(t)
	addPeer(t, client, serverPublic, serverBind.port, "10.97.0.1/32")
	addPeer(t, server, clientPublic, clientBind.port, "10.97.0.2/32")
	payload := sha256.Sum256([]byte("disposable WireGuard encrypted payload"))
	packet := make([]byte, 20+len(payload))
	packet[0] = 0x45
	packet[2] = byte(len(packet) >> 8)
	packet[3] = byte(len(packet))
	packet[8] = 64
	packet[9] = 253
	copy(packet[12:16], net.ParseIP("10.97.0.2").To4())
	copy(packet[16:20], net.ParseIP("10.97.0.1").To4())
	copy(packet[20:], payload[:])
	clientPacket.incoming <- packet
	select {
	case received := <-serverPacket.outgoing:
		if sha256.Sum256(packet) != sha256.Sum256(received) {
			t.Fatal("decrypted payload SHA256 mismatch")
		}
	case <-time.After(5 * time.Second):
		t.Fatal("real WireGuard UDP handshake/payload timed out")
	}
	deadline := time.Now().Add(2 * time.Second)
	for !ready(client) || !ready(server) {
		if time.Now().After(deadline) {
			t.Fatal("fresh handshake proof missing")
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Logf("real UDP WireGuard handshake and packet SHA256: %x", sha256.Sum256(packet))
}
func TestWrongKeyAndUnreachablePeerNeverHaveHandshakeProof(t *testing.T) {
	client, clientBind, _, clientPublic := freshEngine(t)
	server, serverBind, _, _ := freshEngine(t)
	_, wrongPublic := keys(t)
	addPeer(t, client, wrongPublic, serverBind.port, "10.97.0.1/32")
	addPeer(t, server, clientPublic, clientBind.port, "10.97.0.2/32")
	raw, _ := hex.DecodeString(wrongPublic)
	var key device.NoisePublicKey
	copy(key[:], raw)
	client.LookupPeer(key).SendHandshakeInitiation(false)
	blackhole, err := net.ListenPacket("udp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer blackhole.Close()
	unreachable, _, _, _ := freshEngine(t)
	_, unused := keys(t)
	addPeer(t, unreachable, unused, uint16(blackhole.LocalAddr().(*net.UDPAddr).Port), "10.98.0.1/32")
	raw, _ = hex.DecodeString(unused)
	copy(key[:], raw)
	unreachable.LookupPeer(key).SendHandshakeInitiation(false)
	time.Sleep(350 * time.Millisecond)
	if ready(client) || ready(server) || ready(unreachable) {
		t.Fatal("wrong key/unreachable endpoint acquired a handshake")
	}
}
func TestStatusProjectionCannotExposeKeysAndRequiresEveryPeer(t *testing.T) {
	s := handshakeStatus{}
	_, err := io.WriteString(&s, "private_key=synthetic-secret\npreshared_key=synthetic-secret\npublic_key="+strings.Repeat("01", 32)+"\nlast_handshake_time_sec=42\nlast_handshake_time_nsec=123\npublic_key="+strings.Repeat("02", 32)+"\nlast_handshake_time_sec=0\nlast_handshake_time_nsec=0\n")
	if err != nil || s.peers != 2 || s.ready != 1 || len(s.stamps) != 2 || strings.Contains(fmt.Sprintf("%+v", s), "secret") {
		t.Fatal("unsafe handshake projection")
	}
	for _, b := range s.line[:cap(s.line)] {
		if b != 0 {
			t.Fatal("status scratch buffer not cleared")
		}
	}
}

func TestHandshakeStampProjectionHandlesChunksAndRejectsMalformedFields(t *testing.T) {
	s := handshakeStatus{}
	raw := "public_key=" + strings.Repeat("01", 32) + "\nlast_handshake_time_sec=42\nlast_handshake_time_nsec=123\n"
	for _, b := range []byte(raw) {
		if _, err := s.Write([]byte{b}); err != nil {
			t.Fatal(err)
		}
	}
	for _, stamp := range s.stamps {
		if stamp != (handshakeStamp{42, 123}) {
			t.Fatal("lost sub-second handshake proof")
		}
	}
	for _, raw := range []string{
		"public_key=" + strings.Repeat("01", 33) + "\n",
		"public_key=" + strings.Repeat("gg", 32) + "\n",
		"last_handshake_time_sec=invalid\n",
		"last_handshake_time_nsec=123\n",
		"public_key=" + strings.Repeat("01", 32) + "\nlast_handshake_time_nsec=1000000000\n",
	} {
		status := handshakeStatus{}
		if _, err := io.WriteString(&status, raw); err == nil {
			t.Fatal("malformed status accepted")
		}
	}
}
func TestPacketFramesAndStrictControlBounds(t *testing.T) {
	packet := make([]byte, 20)
	packet[0] = 0x45
	packet[3] = 20
	if !validPacket(packet, 1420) || validPacket(append(bytes.Clone(packet), packet...), 1420) || validPacket([]byte{0x45}, 1420) {
		t.Fatal("packet boundary validation")
	}
	packet[0] = 0x44
	if validPacket(packet, 1420) {
		t.Fatal("bad IPv4 header")
	}
	for _, raw := range []string{strings.Repeat(" ", controlLimit+1), `{"sessionId":"00000000-0000-0000-0000-000000000000","generation":1,"execute":"arbitrary"}` + "\n", `{"sessionId":"00000000-0000-0000-0000-000000000000","generation":1}`} {
		control := channel{reader: bufioReader(raw)}
		if _, err := control.read(); err == nil {
			t.Fatal("unbounded/unknown/truncated frame accepted")
		}
	}
	control := channel{reader: bufioReader(`{"sessionId":"00000000-0000-0000-0000-000000000000","generation":2}` + "\n"), session: "00000000-0000-0000-0000-000000000000", generation: 1}
	if _, err := control.read(); err == nil {
		t.Fatal("stale generation")
	}
}
func TestConfigurationRejectsCommandsAndIPv6(t *testing.T) {
	config := configuration{UAPI: "private_key=" + strings.Repeat("01", 32) + "\npublic_key=" + strings.Repeat("02", 32) + "\nendpoint=127.0.0.1:31820\nallowed_ip=10.97.0.0/24\n", NetworkPlan: json.RawMessage(`{"mtu":1420}`), Endpoints: []string{"127.0.0.1:31820"}, PeerCount: 1}
	if _, _, err := validateConfiguration(&config); err != nil {
		t.Fatal(err)
	}
	for _, line := range []string{"fwmark=1\n", "post_up=arbitrary\n", "allowed_ip=::/0\n", "endpoint=192.0.2.1:51820\n"} {
		bad := config
		bad.UAPI += line
		if _, _, err := validateConfiguration(&bad); err == nil {
			t.Fatal("unsafe config accepted")
		}
	}
}

// An opt-in disposable native-test peer. Configuration goes only to the parent pipe.
func TestWindowsPeerFixture(t *testing.T) {
	if os.Getenv("TERMTERM_WG_PEER_FIXTURE") != "1" {
		t.Skip("native integration fixture only")
	}
	server, bind, packet, serverPublic := freshEngine(t)
	clientPrivate, clientPublic := keys(t)
	addPeer(t, server, clientPublic, 31821, "10.97.0.2/32")
	privateBytes, _ := hex.DecodeString(clientPrivate)
	publicBytes, _ := hex.DecodeString(serverPublic)
	profile := fmt.Sprintf("[Interface]\nPrivateKey = %s\nAddress = 10.97.0.2/32\nListenPort = 31821\n[Peer]\nPublicKey = %s\nEndpoint = 127.0.0.1:%d\nAllowedIPs = 10.97.0.1/32\n", base64.StdEncoding.EncodeToString(privateBytes), base64.StdEncoding.EncodeToString(publicBytes), bind.port)
	clear(privateBytes)
	clear(publicBytes)
	json.NewEncoder(os.Stdout).Encode(map[string]any{"config": profile})
	for value := range packet.outgoing {
		if len(value) >= 20 && value[0]>>4 == 4 {
			source := bytes.Clone(value[12:16])
			copy(value[12:16], value[16:20])
			copy(value[16:20], source)
			packet.incoming <- value
		}
	}
}
