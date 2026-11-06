package main

import (
	"errors"
	"golang.org/x/sys/unix"
	"os"
)

func parentLifetime() error {
	parent := os.Getppid()
	if parent == 1 || unix.Prctl(unix.PR_SET_PDEATHSIG, uintptr(unix.SIGKILL), 0, 0, 0) != nil || os.Getppid() != parent {
		return errors.New("parent_lifetime_guard_failed")
	}
	return nil
}
func isolatedLab() error {
	current, err := os.Stat("/proc/self/ns/net")
	if err != nil {
		return err
	}
	initial, err := os.Stat("/proc/1/ns/net")
	if err != nil {
		return err
	}
	if os.Geteuid() != 0 || os.SameFile(current, initial) {
		return errors.New("isolated_lab_namespace_required")
	}
	return nil
}
func verifyBrokerPeer(fd int) error {
	peer, err := unix.GetsockoptUcred(fd, unix.SOL_SOCKET, unix.SO_PEERCRED)
	if err != nil || peer.Uid != 0 || int(peer.Pid) != os.Getppid() {
		return errors.New("broker_peer_identity_failed")
	}
	return nil
}
