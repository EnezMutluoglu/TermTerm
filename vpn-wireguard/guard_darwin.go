package main

import (
	"errors"
	"golang.org/x/sys/unix"
	"os"
)

func parentLifetime() error { return nil }
func isolatedLab() error    { return errors.New("authenticated_broker_required") }
func verifyBrokerPeer(fd int) error {
	peer, err := unix.GetsockoptXucred(fd, unix.SOL_LOCAL, unix.LOCAL_PEERCRED)
	if err != nil || peer.Uid != 0 {
		return errors.New("broker_peer_identity_failed")
	}
	pid, err := unix.GetsockoptInt(fd, unix.SOL_LOCAL, unix.LOCAL_PEERPID)
	if err != nil || pid != os.Getppid() {
		return errors.New("broker_peer_identity_failed")
	}
	return nil
}
