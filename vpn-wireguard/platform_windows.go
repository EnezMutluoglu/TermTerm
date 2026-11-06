package main

import (
	"errors"
	"golang.org/x/sys/windows"
	"io"
	"os"
	"strconv"
	"unsafe"
)

func requireBroker() error {
	var inJob uint32
	result, _, _ := windows.NewLazySystemDLL("kernel32.dll").NewProc("IsProcessInJob").Call(uintptr(windows.CurrentProcess()), 0, uintptr(unsafe.Pointer(&inJob)))
	if result == 0 || inJob == 0 || windows.GetCurrentProcessToken().IsElevated() {
		return errors.New("unelevated_broker_job_required")
	}
	for _, file := range []*os.File{os.Stdin, os.Stdout} {
		kind, err := windows.GetFileType(windows.Handle(file.Fd()))
		if err != nil || kind != windows.FILE_TYPE_PIPE {
			return errors.New("broker_private_pipes_required")
		}
	}
	return nil
}
func openPacket(m message) (io.ReadWriteCloser, error) {
	value, err := strconv.ParseUint(m.PacketHandle, 10, 64)
	if err != nil || value == 0 || m.PacketFD != 0 {
		return nil, errors.New("invalid_packet_handle")
	}
	handle := windows.Handle(value)
	kind, err := windows.GetFileType(handle)
	if err != nil || kind != windows.FILE_TYPE_PIPE {
		return nil, errors.New("packet_pipe_required")
	}
	var mode uint32
	if windows.GetNamedPipeHandleState(handle, &mode, nil, nil, nil, nil, 0) != nil || mode&windows.PIPE_READMODE_MESSAGE == 0 {
		return nil, errors.New("message_packet_pipe_required")
	}
	return os.NewFile(uintptr(handle), "broker-packet"), nil
}
