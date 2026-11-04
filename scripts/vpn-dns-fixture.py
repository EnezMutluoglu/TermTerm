#!/usr/bin/env python3
"""Disposable authoritative DNS fixture; never forwards queries or changes DNS settings."""
import argparse
import ipaddress
import json
import os
from pathlib import Path
import re
import socket
import socketserver
import struct
import threading
from vpn_lab_json import write_json


class ReceiptWriteFailure(RuntimeError):
    pass


def publish_counts(receipt, counts):
    try:
        write_json(receipt, counts)
    except OSError as failure:
        raise ReceiptWriteFailure("dns_receipt_write_failed") from failure


def response(packet, name, address):
    if not 12 <= len(packet) <= 512:
        raise ValueError("invalid_dns_size")
    ident, flags, questions, answers, authority, additional = struct.unpack("!6H", packet[:12])
    if flags & 0xF800 or questions != 1 or answers or authority:
        raise ValueError("invalid_dns_question")
    offset, labels = 12, []
    while True:
        if offset >= len(packet):
            raise ValueError("truncated_dns_name")
        size = packet[offset]
        offset += 1
        if not size:
            break
        if size > 63 or offset + size > len(packet):
            raise ValueError("invalid_dns_label")
        labels.append(packet[offset:offset + size].decode("ascii"))
        offset += size
        if offset - 12 > 254:
            raise ValueError("dns_name_too_long")
    if offset + 4 > len(packet):
        raise ValueError("truncated_dns_type")
    kind, klass = struct.unpack("!HH", packet[offset:offset + 4])
    question = packet[12:offset + 4]
    queried = ".".join(labels).lower()
    known = queried in (name, "tcp." + name)
    category = "expectedFqdnQueries" if known else "bareKnownLabelQueries" if queried == name.split(".")[0] else "otherQueries"
    matched = known and klass == 1
    answer = matched and kind == 1
    header = struct.pack("!6H", ident, 0x8400 | (flags & 0x100) | (0 if matched else 3), 1, int(answer), 0, 0)
    data = b"\xc0\x0c" + struct.pack("!HHIH", 1, 1, 0, 4) + ipaddress.IPv4Address(address).packed if answer else b""
    return header + question + data, answer, category


def serve(address, name, answer, receipt):
    lock = threading.Lock()
    counts = {"udpAnswers": 0, "tcpAnswers": 0, "bareKnownLabelQueries": 0, "expectedFqdnQueries": 0, "otherQueries": 0}

    def reply(packet, transport):
        try:
            result, matched, category = response(packet, name, answer)
        except (ValueError, UnicodeError, struct.error):
            return None
        with lock:
            counts[category] += 1
            if matched:
                counts[transport + "Answers"] += 1
            publish_counts(receipt, counts)
        return result

    class UDP(socketserver.BaseRequestHandler):
        def handle(self):
            packet, connection = self.request
            result = reply(packet, "udp")
            if result is not None:
                connection.sendto(result, self.client_address)

    class TCP(socketserver.BaseRequestHandler):
        def handle(self):
            self.request.settimeout(3)
            def read(size):
                data = b""
                while len(data) < size:
                    block = self.request.recv(size - len(data))
                    if not block:
                        raise ValueError("truncated_tcp_dns")
                    data += block
                return data
            try:
                size = struct.unpack("!H", read(2))[0]
                if not 12 <= size <= 512:
                    return
                result = reply(read(size), "tcp")
                if result is not None:
                    self.request.sendall(struct.pack("!H", len(result)) + result)
            except (OSError, ValueError, struct.error):
                return

    with socketserver.UDPServer((address, 53), UDP) as udp, socketserver.TCPServer((address, 53), TCP) as tcp:
        publish_counts(receipt, counts)
        unhealthy = threading.Event()
        udp.handle_error = tcp.handle_error = lambda request, client_address: unhealthy.set()
        udp.timeout = .1
        worker = threading.Thread(target=tcp.serve_forever, kwargs={"poll_interval": .1}, daemon=True)
        worker.start()
        try:
            while not unhealthy.is_set():
                udp.handle_request()
            raise ReceiptWriteFailure("dns_receipt_write_failed")
        finally:
            tcp.shutdown()
            worker.join(1)


def self_test():
    name = "probe.ab12.termterm.test"
    wire = b"".join(bytes([len(label)]) + label.encode() for label in name.split(".")) + b"\0"
    def query(kind=1, question=wire):
        return struct.pack("!6H", 123, 0x100, 1, 0, 0, 0) + question + struct.pack("!HH", kind, 1)
    result, matched, category = response(query(), name, "10.88.0.53")
    assert matched and struct.unpack("!6H", result[:12]) == (123, 0x8500, 1, 1, 0, 0)
    assert category == "expectedFqdnQueries"
    assert response(query(question=b"\x05probe\0"), name, "10.88.0.53")[2] == "bareKnownLabelQueries"
    assert response(query(), "different.termterm.test", "10.88.0.53")[2] == "otherQueries"
    assert result[-4:] == socket.inet_aton("10.88.0.53") and result[-10:-6] == b"\0\0\0\0"
    assert not response(query(28), name, "10.88.0.53")[1]
    tcp_wire = b"\x03tcp" + wire
    assert response(query(question=tcp_wire), name, "10.88.0.53")[1]
    assert response(query(), "different.termterm.test", "10.88.0.53")[0][3] & 15 == 3
    for packet in [b"", query(question=b"\xc0\x0c"), query()[:-3], query(question=b"\x3fshort")]:
        try:
            response(packet, name, "10.88.0.53")
        except (ValueError, UnicodeError):
            pass
        else:
            raise AssertionError("malformed_dns_accepted")
    print("PASS: DNS A/AAAA/NXDOMAIN, TCP name, TTL0, safe query categories and malformed question guards (12 checks)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--name")
    parser.add_argument("--receipt", type=Path)
    args = parser.parse_args()
    if args.self_test:
        self_test()
    else:
        if not args.name or not re.fullmatch(r"probe\.[0-9a-f]{16}\.termterm\.test", args.name) or not args.receipt:
            parser.error("Use a generated disposable fixture name and private receipt path")
        if os.name != "posix" or os.readlink("/proc/self/ns/net") == os.readlink("/proc/1/ns/net"):
            parser.error("Use the disposable server network namespace")
        serve("10.88.0.53", args.name, "10.88.0.53", args.receipt)
