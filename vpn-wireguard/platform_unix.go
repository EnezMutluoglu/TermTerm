//go:build linux || darwin

package main

import (
	"errors"
	"golang.org/x/sys/unix"
	"io"
	"net"
	"os"
	"strconv"
	"time"
)

func requireBroker() error {
	for _, file := range []*os.File{os.Stdin, os.Stdout} {
		info, err := file.Stat()
		if err != nil || info.Mode()&os.ModeNamedPipe == 0 {
			return errors.New("broker_private_pipes_required")
		}
	}
	if err := parentLifetime(); err != nil {
		return err
	}
	value := os.Getenv("TERMTERM_VPN_BROKER_FD")
	if value == "" {
		return isolatedLab()
	}
	fd, err := strconv.Atoi(value)
	if err != nil || fd < 3 || os.Geteuid() != 0 {
		return errors.New("invalid_broker_capability")
	}
	if err = verifyBrokerPeer(fd); err != nil {
		return err
	}
	file := os.NewFile(uintptr(fd), "broker-capability")
	stream, err := net.FileConn(file)
	file.Close()
	if err != nil {
		return errors.New("invalid_broker_capability")
	}
	stream.SetReadDeadline(time.Now().Add(time.Second))
	magic := make([]byte, len("TERMTERM-VPN-BROKER/1\n"))
	if _, err = io.ReadFull(stream, magic); err != nil || string(magic) != "TERMTERM-VPN-BROKER/1\n" {
		stream.Close()
		return errors.New("invalid_broker_capability")
	}
	stream.SetReadDeadline(time.Time{})
	go func() { io.Copy(io.Discard, stream); os.Exit(1) }()
	return nil
}
func openPacket(m message) (io.ReadWriteCloser, error) {
	if m.PacketFD < 3 || m.PacketHandle != "" {
		return nil, errors.New("invalid_packet_fd")
	}
	kind, err := unix.GetsockoptInt(m.PacketFD, unix.SOL_SOCKET, unix.SO_TYPE)
	if err != nil || kind != unix.SOCK_DGRAM {
		return nil, errors.New("packet_datagram_socket_required")
	}
	file := os.NewFile(uintptr(m.PacketFD), "broker-packet")
	packet, err := net.FileConn(file)
	file.Close()
	if err != nil {
		return nil, errors.New("invalid_packet_fd")
	}
	return packet, nil
}
