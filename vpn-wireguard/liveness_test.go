package main

import (
	"encoding/hex"
	"testing"
	"time"

	"golang.zx2c4.com/wireguard/device"
)

func TestLivenessIdleRecoveryAndEveryPeerDeadline(t *testing.T) {
	key1, key2 := device.NoisePublicKey{1}, device.NoisePublicKey{2}
	stamps := map[device.NoisePublicKey]handshakeStamp{key1: {100, 1}, key2: {100, 2}}
	now := time.Now()
	liveness := newPeerLiveness(stamps, now)
	probes := map[device.NoisePublicKey]int{}
	probe := func(key device.NoisePublicKey) { probes[key]++ }
	for second := 1; second <= 90; second++ {
		// Idle peers answer probes even without tunnel payload or persistent keepalive.
		if second%16 == 0 {
			for key, stamp := range stamps {
				stamp.nanos++
				stamps[key] = stamp
			}
		}
		if err := liveness.check(stamps, now.Add(time.Duration(second)*time.Second), probe); err != nil {
			t.Fatal("idle connection failed", err)
		}
	}
	if probes[key1] < 5 || probes[key2] < 5 {
		t.Fatal("idle peers were not explicitly probed")
	}
	// Only peer 2 blackholes; a fresh handshake on peer 1 cannot hide it.
	for second := 91; second <= 125; second++ {
		stamp := stamps[key1]
		stamp.nanos++
		stamps[key1] = stamp
		err := liveness.check(stamps, now.Add(time.Duration(second)*time.Second), probe)
		if second < 125 && err != nil {
			t.Fatal("failed before the recovery window", err)
		}
		if second == 125 && (err == nil || err.Error() != "wireguard_peer_unresponsive") {
			t.Fatal("missing bounded per-peer failure", err)
		}
	}
}

func TestLivenessClockChangesSleepAndShortDrop(t *testing.T) {
	key := device.NoisePublicKey{1}
	stamps := map[device.NoisePublicKey]handshakeStamp{key: {100, 1}}
	now := time.Now()
	liveness := newPeerLiveness(stamps, now)
	probes := 0
	probe := func(device.NoisePublicKey) { probes++ }
	for second := 1; second <= 35; second++ {
		if err := liveness.check(stamps, now.Add(time.Duration(second)*time.Second), probe); err != nil {
			t.Fatal(err)
		}
	}
	// A completed handshake counts even if the system wall clock moved backwards.
	stamps[key] = handshakeStamp{50, 1}
	if err := liveness.check(stamps, now.Add(36*time.Second), probe); err != nil {
		t.Fatal("short loss did not recover", err)
	}
	if err := liveness.check(stamps, now.Add(time.Hour), probe); err != nil || probes != 3 {
		t.Fatal("resume did not issue a probe with grace", err, probes)
	}
	for second := 1; second <= 30; second++ {
		err := liveness.check(stamps, now.Add(time.Hour+time.Duration(second)*time.Second), probe)
		if second < 30 && err != nil {
			t.Fatal("resume grace ended too early", err)
		}
		if second == 30 && err == nil {
			t.Fatal("resumed blackhole never failed")
		}
	}
}

func TestLivenessMissingRequiredPeerFailsClosed(t *testing.T) {
	now := time.Now()
	key := device.NoisePublicKey{1}
	liveness := newPeerLiveness(map[device.NoisePublicKey]handshakeStamp{key: {100, 1}}, now)
	if err := liveness.check(nil, now, func(device.NoisePublicKey) { t.Fatal("missing peer was probed") }); err == nil || err.Error() != "wireguard_status_failed" {
		t.Fatal("missing required peer did not fail closed", err)
	}
}

func TestLivenessRepeatedSchedulingGapsCannotExtendGraceForever(t *testing.T) {
	now := time.Now()
	key := device.NoisePublicKey{1}
	stamps := map[device.NoisePublicKey]handshakeStamp{key: {100, 1}}
	liveness := newPeerLiveness(stamps, now)
	for second := 6; second <= 36; second += 6 {
		err := liveness.check(stamps, now.Add(time.Duration(second)*time.Second), func(device.NoisePublicKey) {})
		if second < 36 && err != nil {
			t.Fatal("first scheduling gap did not get recovery grace", err)
		}
		if second == 36 && err == nil {
			t.Fatal("repeated scheduling gaps kept an unresponsive peer alive")
		}
	}
}

func TestExplicitHandshakeProbeRefreshesIdlePeerProof(t *testing.T) {
	client, clientBind, _, clientPublic := freshEngine(t)
	server, serverBind, _, serverPublic := freshEngine(t)
	addPeer(t, client, serverPublic, serverBind.port, "10.97.0.1/32")
	addPeer(t, server, clientPublic, clientBind.port, "10.97.0.2/32")
	var key device.NoisePublicKey
	hex.Decode(key[:], []byte(serverPublic))
	peer := client.LookupPeer(key)
	if err := peer.SendHandshakeInitiation(false); err != nil {
		t.Fatal(err)
	}
	var initial handshakeStamp
	deadline := time.Now().Add(3 * time.Second)
	for {
		status := handshakeStatus{}
		if err := client.IpcGetOperation(&status); err != nil {
			t.Fatal(err)
		}
		initial = status.stamps[key]
		if initial.seconds > 0 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("initial idle handshake timed out")
		}
		time.Sleep(10 * time.Millisecond)
	}
	// The upstream API rate-limits initiations for RekeyTimeout after the last send.
	time.Sleep(device.RekeyTimeout)
	if err := peer.SendHandshakeInitiation(false); err != nil {
		t.Fatal(err)
	}
	deadline = time.Now().Add(3 * time.Second)
	for {
		status := handshakeStatus{}
		if err := client.IpcGetOperation(&status); err != nil {
			t.Fatal(err)
		}
		if status.stamps[key] != initial {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("explicit idle probe produced no fresh proof")
		}
		time.Sleep(10 * time.Millisecond)
	}
}
