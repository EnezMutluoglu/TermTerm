// TermTerm's packet-only adapter around the official WireGuard engine.
package main

import (
	"bufio"
	"bytes"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/netip"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"golang.zx2c4.com/wireguard/conn"
	"golang.zx2c4.com/wireguard/device"
	"golang.zx2c4.com/wireguard/tun"
)

const controlLimit = 2 * 1024 * 1024

type configuration struct {
	UAPI        string          `json:"uapi"`
	NetworkPlan json.RawMessage `json:"networkPlan"`
	Endpoints   []string        `json:"endpoints"`
	PeerCount   int             `json:"peerCount"`
}
type message struct {
	SessionID    string         `json:"sessionId"`
	Generation   uint64         `json:"generation"`
	WireGuard    *configuration `json:"wireguard,omitempty"`
	Action       string         `json:"action,omitempty"`
	PacketHandle string         `json:"packetHandle,omitempty"`
	PacketFD     int            `json:"packetFd,omitempty"`
}
type channel struct {
	reader     *bufio.Reader
	session    string
	generation uint64
}

func (c *channel) read() (message, error) {
	var frame []byte
	defer func() { clear(frame) }()
	for {
		part, err := c.reader.ReadSlice('\n')
		if len(frame)+len(part) > controlLimit {
			return message{}, errors.New("control_message_too_large")
		}
		frame = append(frame, part...)
		if err == nil {
			break
		}
		if !errors.Is(err, bufio.ErrBufferFull) {
			return message{}, errors.New("control_channel_closed")
		}
	}
	var m message
	decoder := json.NewDecoder(bytes.NewReader(frame))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&m); err != nil {
		return m, errors.New("invalid_control_message")
	}
	if decoder.Decode(new(any)) != io.EOF {
		return m, errors.New("invalid_control_message")
	}
	if len(m.SessionID) != 36 || m.Generation == 0 || m.Generation >= 1<<53 {
		return m, errors.New("invalid_session_generation")
	}
	if c.session != "" && (m.SessionID != c.session || m.Generation != c.generation) {
		return m, errors.New("stale_control_generation")
	}
	return m, nil
}
func (c *channel) event(kind string, detail any) error {
	return json.NewEncoder(os.Stdout).Encode(map[string]any{"kind": kind, "detail": detail, "sessionId": c.session, "generation": c.generation})
}

// WireGuard never opens an adapter or alters routes/DNS: the broker owns this endpoint.
type packetDevice struct {
	packet io.ReadWriteCloser
	mtu    int
	events chan tun.Event
	once   sync.Once
	write  sync.Mutex
}

func newPacketDevice(packet io.ReadWriteCloser, mtu int) *packetDevice {
	return &packetDevice{packet: packet, mtu: mtu, events: make(chan tun.Event)}
}
func (t *packetDevice) File() *os.File           { return nil }
func (t *packetDevice) Name() (string, error)    { return "termterm-broker-packet", nil }
func (t *packetDevice) MTU() (int, error)        { return t.mtu, nil }
func (t *packetDevice) Events() <-chan tun.Event { return t.events }
func (t *packetDevice) BatchSize() int           { return 1 }
func (t *packetDevice) Close() error {
	var err error
	t.once.Do(func() { err = t.packet.Close(); close(t.events) })
	return err
}
func validPacket(p []byte, mtu int) bool {
	if len(p) < 20 || len(p) > mtu {
		return false
	}
	switch p[0] >> 4 {
	case 4:
		h := int(p[0]&15) * 4
		return h >= 20 && h <= len(p) && int(p[2])<<8|int(p[3]) == len(p)
	case 6:
		return len(p) >= 40 && (int(p[4])<<8|int(p[5]))+40 == len(p)
	}
	return false
}
func (t *packetDevice) Read(bufs [][]byte, sizes []int, offset int) (int, error) {
	if len(bufs) == 0 || len(sizes) == 0 || offset < 0 || len(bufs[0])-offset < t.mtu {
		return 0, errors.New("invalid_packet_buffer")
	}
	// Read the whole message so an oversized packet cannot be split into valid packets.
	var packet [65536]byte
	n, err := t.packet.Read(packet[:])
	if err != nil {
		return 0, err
	}
	if !validPacket(packet[:n], t.mtu) {
		return 0, errors.New("invalid_packet_frame")
	}
	copy(bufs[0][offset:], packet[:n])
	sizes[0] = n
	return 1, nil
}
func (t *packetDevice) Write(bufs [][]byte, offset int) (int, error) {
	t.write.Lock()
	defer t.write.Unlock()
	for i, p := range bufs {
		if offset < 0 || offset > len(p) || !validPacket(p[offset:], t.mtu) {
			return i, errors.New("invalid_packet_frame")
		}
		n, err := t.packet.Write(p[offset:])
		if err != nil {
			return i, err
		}
		if n != len(p)-offset {
			return i, io.ErrShortWrite
		}
	}
	return len(bufs), nil
}

// Endpoint roaming stays within the gateway addresses reserved before route setup.
type pinnedBind struct {
	conn.Bind
	allowed map[string]bool
}

func (b *pinnedBind) ParseEndpoint(value string) (conn.Endpoint, error) {
	if !b.allowed[value] {
		return nil, errors.New("unpinned_endpoint")
	}
	return b.Bind.ParseEndpoint(value)
}
func (b *pinnedBind) Send(bufs [][]byte, endpoint conn.Endpoint) error {
	if !b.allowed[endpoint.DstToString()] {
		return errors.New("unpinned_endpoint")
	}
	return b.Bind.Send(bufs, endpoint)
}
func (b *pinnedBind) Open(port uint16) ([]conn.ReceiveFunc, uint16, error) {
	fns, port, err := b.Bind.Open(port)
	for i, fn := range fns {
		fns[i] = func(packets [][]byte, sizes []int, endpoints []conn.Endpoint) (int, error) {
			n, err := fn(packets, sizes, endpoints)
			for j := 0; j < n; j++ {
				if endpoints[j] == nil || !b.allowed[endpoints[j].DstToString()] {
					sizes[j] = 0
				}
			}
			return n, err
		}
	}
	return fns, port, err
}
func validateConfiguration(config *configuration) (int, []device.NoisePublicKey, error) {
	if config == nil || len(config.UAPI) == 0 || len(config.UAPI) > controlLimit || config.PeerCount < 1 || config.PeerCount > 64 || len(config.Endpoints) != config.PeerCount {
		return 0, nil, errors.New("invalid_wireguard_configuration")
	}
	for _, value := range config.Endpoints {
		endpoint, err := netip.ParseAddrPort(value)
		if err != nil || !endpoint.Addr().Is4() || endpoint.Port() == 0 || endpoint.Addr().IsUnspecified() || endpoint.Addr().IsMulticast() {
			return 0, nil, errors.New("invalid_wireguard_endpoint")
		}
	}
	var plan struct {
		MTU int `json:"mtu"`
	}
	if json.Unmarshal(config.NetworkPlan, &plan) != nil || plan.MTU < 576 || plan.MTU > 9000 {
		return 0, nil, errors.New("invalid_wireguard_mtu")
	}
	peers := []device.NoisePublicKey{}
	allowed := map[string]bool{}
	for _, endpoint := range config.Endpoints {
		allowed[endpoint] = true
	}
	for _, line := range strings.Split(config.UAPI, "\n") {
		if line == "" {
			continue
		}
		name, value, ok := strings.Cut(line, "=")
		if !ok {
			return 0, nil, errors.New("invalid_wireguard_uapi")
		}
		switch name {
		case "private_key", "preshared_key", "public_key":
			raw, err := hex.DecodeString(value)
			if err != nil || len(raw) != 32 {
				return 0, nil, errors.New("invalid_wireguard_key")
			}
			if name == "public_key" {
				var key device.NoisePublicKey
				copy(key[:], raw)
				peers = append(peers, key)
			}
			clear(raw)
		case "endpoint":
			if !allowed[value] {
				return 0, nil, errors.New("unpinned_endpoint")
			}
		case "allowed_ip":
			prefix, err := netip.ParsePrefix(value)
			if err != nil || !prefix.Addr().Is4() {
				return 0, nil, errors.New("wireguard_ipv6_unsupported")
			}
		case "listen_port", "persistent_keepalive_interval":
			if _, err := strconv.ParseUint(value, 10, 16); err != nil {
				return 0, nil, errors.New("invalid_wireguard_port_or_keepalive")
			}
		case "replace_peers", "replace_allowed_ips":
			if value != "true" {
				return 0, nil, errors.New("invalid_wireguard_uapi")
			}
		default:
			return 0, nil, errors.New("unsupported_wireguard_uapi")
		}
	}
	if len(peers) != config.PeerCount {
		return 0, nil, errors.New("invalid_wireguard_peer_count")
	}
	return plan.MTU, peers, nil
}

type handshakeStatus struct {
	line         []byte
	peers, ready int
	key          device.NoisePublicKey
	stamp        handshakeStamp
	stamps       map[device.NoisePublicKey]handshakeStamp
}

func (s *handshakeStatus) Write(data []byte) (int, error) {
	for _, b := range data {
		if b == '\n' {
			if bytes.HasPrefix(s.line, []byte("public_key=")) {
				s.peers++
				if s.stamps == nil {
					s.stamps = make(map[device.NoisePublicKey]handshakeStamp)
				}
				value := s.line[len("public_key="):]
				if len(value) != hex.EncodedLen(len(s.key)) {
					return 0, errors.New("invalid_wireguard_status")
				}
				if _, err := hex.Decode(s.key[:], value); err != nil {
					return 0, errors.New("invalid_wireguard_status")
				}
			}
			if value, ok := bytes.CutPrefix(s.line, []byte("last_handshake_time_sec=")); ok {
				n, err := strconv.ParseInt(string(value), 10, 64)
				if err != nil {
					return 0, errors.New("invalid_wireguard_status")
				}
				s.stamp.seconds = n
				if n > 0 {
					s.ready++
				}
			}
			if value, ok := bytes.CutPrefix(s.line, []byte("last_handshake_time_nsec=")); ok {
				n, err := strconv.ParseInt(string(value), 10, 64)
				if err != nil || n < 0 || n >= int64(time.Second) || s.stamps == nil {
					return 0, errors.New("invalid_wireguard_status")
				}
				s.stamp.nanos = n
				s.stamps[s.key] = s.stamp
			}
			clear(s.line)
			s.line = s.line[:0]
		} else {
			if len(s.line) >= 1024 {
				return 0, errors.New("invalid_wireguard_status")
			}
			s.line = append(s.line, b)
		}
	}
	return len(data), nil
}

func finishRun(result error, report func(string, any) error, cleanup func()) {
	if result != nil {
		report("failed", result.Error())
	}
	cleanup()
}

func run() (result error) {
	if err := requireBroker(); err != nil {
		return err
	}
	control := channel{reader: bufio.NewReaderSize(os.Stdin, 65536)}
	initial, err := control.read()
	if err != nil {
		return err
	}
	control.session = initial.SessionID
	control.generation = initial.Generation
	var tun *packetDevice
	var engine *device.Device
	defer func() {
		// Publish the terminal reason before cleanup closes the separate packet pipe.
		finishRun(result, control.event, func() {
			if engine != nil {
				engine.Close()
			}
			if tun != nil {
				tun.Close()
			}
		})
	}()
	mtu, peers, err := validateConfiguration(initial.WireGuard)
	if err != nil {
		return err
	}
	config := initial.WireGuard
	var plan any
	if json.Unmarshal(config.NetworkPlan, &plan) != nil {
		return errors.New("invalid_network_plan")
	}
	if err = control.event("network-plan", plan); err != nil {
		return err
	}
	accepted, err := control.read()
	if err != nil || accepted.Action != "accept-network" {
		return errors.New("network_not_accepted")
	}
	if err = control.event("tunnel-ready", plan); err != nil {
		return err
	}
	configured, err := control.read()
	if err != nil || configured.Action != "network-configured" {
		return errors.New("network_not_configured")
	}
	packet, err := openPacket(configured)
	if err != nil {
		return err
	}
	tun = newPacketDevice(packet, mtu)
	allowed := map[string]bool{}
	for _, endpoint := range config.Endpoints {
		allowed[endpoint] = true
	}
	// Never send upstream logs or UAPI replies (which contain private keys) to the UI.
	engine = device.NewDevice(tun, &pinnedBind{Bind: conn.NewDefaultBind(), allowed: allowed}, &device.Logger{Verbosef: func(string, ...any) {}, Errorf: func(string, ...any) {}})
	if err = engine.IpcSet(config.UAPI); err != nil {
		return errors.New("wireguard_configuration_rejected")
	}
	config.UAPI = ""
	initial.WireGuard = nil
	if err = engine.Up(); err != nil {
		return errors.New("wireguard_udp_bind_failed")
	}
	for _, key := range peers {
		peer := engine.LookupPeer(key)
		if peer == nil {
			return errors.New("wireguard_peer_not_configured")
		}
		if err = peer.SendHandshakeInitiation(false); err != nil {
			return errors.New("wireguard_handshake_send_failed")
		}
	}
	if err = control.event("progress", map[string]any{"name": "awaiting-peer-handshake", "error": false}); err != nil {
		return err
	}
	requests := make(chan message, 1)
	failure := make(chan error, 1)
	go func() {
		for {
			m, err := control.read()
			if err != nil {
				failure <- err
				return
			}
			requests <- m
		}
	}()
	deadline := time.NewTimer(30 * time.Second)
	defer deadline.Stop()
	tick := time.NewTicker(100 * time.Millisecond)
	defer tick.Stop()
	connected := false
	var liveness *peerLiveness
	for {
		select {
		case <-engine.Wait():
			return errors.New("wireguard_packet_channel_closed")
		case <-failure:
			return errors.New("control_channel_closed")
		case m := <-requests:
			if m.Action != "disconnect" {
				return errors.New("unexpected_control_action")
			}
			return control.event("disconnected", map[string]any{})
		case <-deadline.C:
			if !connected {
				return errors.New("wireguard_peer_handshake_timeout")
			}
		case <-tick.C:
			status := handshakeStatus{}
			if err := engine.IpcGetOperation(&status); err != nil || status.peers != config.PeerCount || len(status.stamps) != config.PeerCount {
				return errors.New("wireguard_status_failed")
			}
			now := time.Now()
			if !connected {
				if status.peers == config.PeerCount && status.ready == config.PeerCount {
					connected = true
					liveness = newPeerLiveness(status.stamps, now)
					deadline.Stop()
					if err = control.event("connected", map[string]any{"peers": status.ready}); err != nil {
						return err
					}
				}
			} else if err := liveness.check(status.stamps, now, func(key device.NoisePublicKey) {
				if peer := engine.LookupPeer(key); peer != nil {
					// Transient send failures receive the same bounded recovery window as packet loss.
					peer.SendHandshakeInitiation(false)
				}
			}); err != nil {
				return err
			}
		}
	}
}
func main() {
	if err := run(); err != nil { // Error strings never contain configuration or upstream diagnostics.
		fmt.Fprintln(os.Stderr, err.Error())
		os.Exit(1)
	}
}
