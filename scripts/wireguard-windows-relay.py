#!/usr/bin/env python3
"""Unelevated loopback-only ciphertext relay for the disposable Linux kernel peer."""
import argparse
import ipaddress
import json
import os
from pathlib import Path
import selectors
import socket
import time
from vpn_lab_json import read_json, write_json

ROOT = Path(__file__).resolve().parents[1]


def fixture_path(path):
    path = path.resolve(strict=True)
    if not path.is_relative_to((ROOT / ".lab").resolve()) or path.name != "profiles.json":
        raise ValueError("Use the newly generated private WireGuard lab profile")
    return path


def upstream(value):
    address = ipaddress.ip_address(value)
    if address.version != 4 or not any(address in ipaddress.ip_network(prefix) for prefix in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"]):
        raise ValueError("A private WSL IPv4 endpoint is required")
    return str(address), 31820


def write_receipt(path, value):
    write_json(path, value)


def loss_control(directory, fixture_id):
    path = directory / "relay-loss.json"
    value = read_json(path)
    if value.get("fixtureId") != fixture_id or type(value.get("generation")) is not int or value["generation"] < 0 or value.get("direction") not in ["none", "both", "outbound", "inbound"]:
        raise ValueError("Loss control escaped the pinned disposable fixture")
    return value


def initialize_loss_control(directory, fixture_id):
    try:
        write_json(directory / "relay-loss.json", {"fixtureId": fixture_id, "generation": 0, "direction": "none"}, exclusive=True)
    except FileExistsError:
        pass
    return loss_control(directory, fixture_id)


def serve(path, duration):
    path = fixture_path(path)
    directory = path.parent
    fixture = read_json(path)
    destination = upstream(fixture["kernelPeer"]["address"])
    ready = read_json(directory / "ready.json")
    if ready["fixtureId"] != fixture["fixtureId"] or ready["peerPort"] != 31820:
        raise ValueError("Kernel fixture identity changed")
    receipt = {"version": 1, "fixtureId": fixture["fixtureId"], "pid": os.getpid(), "loopbackOnly": True,
               "forwardedDatagrams": 0, "returnedDatagrams": 0, "droppedOutbound": 0, "droppedInbound": 0, "stopped": False,
               "lossControl": initialize_loss_control(directory, fixture["fixtureId"])}
    selector = selectors.DefaultSelector()
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as local, socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as remote:
        if os.name == "nt":
            local.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        local.bind(("127.0.0.1", 31820))
        remote.connect(destination)
        selector.register(local, selectors.EVENT_READ)
        selector.register(remote, selectors.EVENT_READ)
        client = None
        deadline = time.monotonic() + duration
        receipt_path = directory / "relay-receipt.json"
        write_receipt(receipt_path, receipt)
        try:
            while time.monotonic() < deadline and not any((directory / name).exists() for name in ["stop", "relay-stop", "stopped.json"]):
                loss = loss_control(directory, fixture["fixtureId"])
                receipt["lossControl"] = loss
                for key, _ in selector.select(.25):
                    if key.fileobj is local:
                        try:
                            packet, client = local.recvfrom(65536)
                        except ConnectionResetError:
                            # Windows reports ICMP from the previous closed client on the next receive.
                            client = None
                            continue
                        if client[0] != "127.0.0.1" or not 32 <= len(packet) <= 65535:
                            continue
                        if loss["direction"] in ["both", "outbound"]:
                            receipt["droppedOutbound"] += 1
                        else:
                            remote.send(packet)
                            receipt["forwardedDatagrams"] += 1
                    else:
                        packet = remote.recv(65536)
                        if client and 32 <= len(packet) <= 65535:
                            if loss["direction"] in ["both", "inbound"]:
                                receipt["droppedInbound"] += 1
                            else:
                                local.sendto(packet, client)
                                receipt["returnedDatagrams"] += 1
                write_receipt(receipt_path, receipt)
        finally:
            receipt["stopped"] = True
            write_receipt(receipt_path, receipt)
            selector.close()


def self_test():
    import errno
    import tempfile
    from unittest.mock import MagicMock, patch
    assert upstream("172.22.42.100") == ("172.22.42.100", 31820)
    for value in ["8.8.8.8", "127.0.0.1", "169.254.1.1", "::1", "vpn.example"]:
        try:
            upstream(value)
        except ValueError:
            pass
        else:
            raise AssertionError("Public, loopback or nonnumeric upstream accepted")
    with tempfile.TemporaryDirectory() as temporary:
        directory = Path(temporary)
        assert initialize_loss_control(directory, "fixture") == {"fixtureId": "fixture", "generation": 0, "direction": "none"}
        path = directory / "relay-loss.json"
        original, metadata = path.read_bytes(), path.stat()
        assert initialize_loss_control(directory, "fixture")["generation"] == 0
        assert path.read_bytes() == original and (path.stat().st_mtime_ns, path.stat().st_mode) == (metadata.st_mtime_ns, metadata.st_mode)
        for direction in ["none", "both", "outbound", "inbound"]:
            write_receipt(directory / "relay-loss.json", {"fixtureId": "fixture", "generation": 1, "direction": direction})
            assert loss_control(directory, "fixture")["direction"] == direction
        valid = json.dumps({"fixtureId": "fixture", "generation": 7, "direction": "both"}).encode()
        for failure in [PermissionError(errno.EACCES, "replacement busy"), FileNotFoundError(errno.ENOENT, "replacement busy")]:
            with patch.object(Path, "read_bytes", side_effect=[failure, valid]) as read, patch("time.sleep") as sleep:
                assert loss_control(directory, "fixture") == json.loads(valid)
                assert read.call_count == 2 and sleep.call_count == 1
            with patch.object(Path, "read_bytes", side_effect=failure) as read, patch("time.sleep") as sleep:
                try:
                    loss_control(directory, "fixture")
                except type(failure):
                    assert read.call_count == 100 and sleep.call_count == 99
                else:
                    raise AssertionError("Persistent control read failure did not fail within its bound")
        for value in [{"fixtureId": "other", "generation": 1, "direction": "both"}, {"fixtureId": "fixture", "generation": True, "direction": "both"}, {"fixtureId": "fixture", "generation": 1, "direction": "other"}]:
            write_receipt(directory / "relay-loss.json", value)
            with patch("time.sleep") as sleep:
                try:
                    initialize_loss_control(directory, "fixture")
                except ValueError:
                    sleep.assert_not_called()
                    assert json.loads(path.read_bytes()) == value
                else:
                    raise AssertionError("Invalid existing loss control was accepted or overwritten")
        with patch.object(Path, "read_bytes", return_value=b"{") as read, patch("time.sleep") as sleep:
            try:
                loss_control(directory, "fixture")
            except json.JSONDecodeError:
                assert read.call_count == 1
                sleep.assert_not_called()
            else:
                raise AssertionError("Malformed loss control accepted")
        profile = directory / "profiles.json"
        profile.write_text(json.dumps({"fixtureId": "fixture", "kernelPeer": {"address": "172.22.42.100"}}))
        (directory / "ready.json").write_text(json.dumps({"fixtureId": "fixture", "peerPort": 31820}))
        local, remote, selector, receipts = MagicMock(), MagicMock(), MagicMock(), []
        local.__enter__.return_value, remote.__enter__.return_value = local, remote
        original_read = Path.read_bytes
        control_reads = 0

        def failed_control_read(path):
            nonlocal control_reads
            if path.name != "relay-loss.json":
                return original_read(path)
            control_reads += 1
            if control_reads == 1:
                return valid
            raise PermissionError(errno.EACCES, "persistent replacement lock")

        def assert_waiting(_):
            selector.select.assert_not_called()
            local.recvfrom.assert_not_called()
            remote.send.assert_not_called()
            assert len(receipts) == 1 and receipts[0]["lossControl"]["generation"] == 7

        with patch.dict(globals(), {"fixture_path": lambda path: path, "write_receipt": lambda path, value: receipts.append(json.loads(json.dumps(value)))}), patch.object(Path, "read_bytes", failed_control_read), patch.object(socket, "socket", side_effect=[local, remote]), patch.object(selectors, "DefaultSelector", return_value=selector), patch("time.sleep", side_effect=assert_waiting):
            try:
                serve(profile, 60)
            except PermissionError:
                assert control_reads == 101 and len(receipts) == 2 and receipts[-1]["stopped"]
                assert receipts[-1]["lossControl"]["generation"] == 7
                selector.select.assert_not_called()
                local.sendto.assert_not_called()
                remote.send.assert_not_called()
            else:
                raise AssertionError("Failed control read forwarded packets or acknowledged a fallback generation")
        with patch("vpn_lab_json.replace", side_effect=[PermissionError(errno.EACCES, "reader busy"), None]) as replace:
            write_receipt(directory / "race.json", {})
            assert replace.call_count == 2
        with patch("vpn_lab_json.replace", side_effect=PermissionError(errno.EACCES, "persistent lock")), patch("time.sleep") as sleep:
            try:
                write_receipt(directory / "race.json", {})
            except PermissionError:
                assert sleep.call_count == 99
            else:
                raise AssertionError("Persistent receipt lock did not fail within its bound")
    print("PASS: relay control initialization, bounded reads, generation/identity validation and no forwarding during read failure; no sockets opened")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path)
    parser.add_argument("--duration", type=int, default=7200)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if not args.fixture or not 60 <= args.duration <= 14400:
            parser.error("Use a generated fixture and a bounded 60–14400 second lifetime")
        serve(args.fixture, args.duration)
