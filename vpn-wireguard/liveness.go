package main

import (
	"errors"
	"time"

	"golang.zx2c4.com/wireguard/device"
)

const (
	peerProbeInterval = 15 * time.Second
	peerProofTimeout  = 45 * time.Second
	peerPollGap       = 5 * time.Second
)

type handshakeStamp struct{ seconds, nanos int64 }

type peerProof struct {
	stamp        handshakeStamp
	seen, probed time.Time
	resumed      bool
}

type peerLiveness struct {
	peers   map[device.NoisePublicKey]peerProof
	checked time.Time
}

func newPeerLiveness(stamps map[device.NoisePublicKey]handshakeStamp, now time.Time) *peerLiveness {
	l := &peerLiveness{peers: make(map[device.NoisePublicKey]peerProof), checked: now}
	for key, stamp := range stamps {
		l.peers[key] = peerProof{stamp: stamp, seen: now, probed: now}
	}
	return l
}

// Only authenticated completed handshakes prove reachability; keepalives need no reply.
func (l *peerLiveness) check(stamps map[device.NoisePublicKey]handshakeStamp, now time.Time, probe func(device.NoisePublicKey)) error {
	paused := now.Sub(l.checked) > peerPollGap
	l.checked = now
	for key, proof := range l.peers {
		stamp, ok := stamps[key]
		if !ok {
			return errors.New("wireguard_status_failed")
		}
		if stamp != proof.stamp {
			proof = peerProof{stamp: stamp, seen: now, probed: now}
		} else if paused && !proof.resumed {
			// Give a resumed process time to exchange a fresh probe before failing.
			proof.seen = now.Add(-peerProbeInterval)
			proof.probed = proof.seen
			proof.resumed = true
		}
		if now.Sub(proof.seen) >= peerProofTimeout {
			return errors.New("wireguard_peer_unresponsive")
		}
		if now.Sub(proof.probed) >= peerProbeInterval {
			probe(key)
			proof.probed = now
		}
		l.peers[key] = proof
	}
	return nil
}
