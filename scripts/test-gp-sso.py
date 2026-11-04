#!/usr/bin/env python3
"""Exercise the production OpenConnect public SSO API against disposable local TLS."""
import argparse
import base64
import ctypes as C
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import importlib.util
import ipaddress
import os
from pathlib import Path
import ssl
import socket
import subprocess
import tempfile
import threading
import time
from types import SimpleNamespace
from urllib.parse import parse_qs

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / ".tools/vpn-windows"
CERT_CB = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_char_p)
FORM_CB = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_void_p)
LOG_CB = C.CFUNCTYPE(None, C.c_void_p, C.c_int, C.c_char_p)
BROWSER_CB = C.CFUNCTYPE(C.c_int, C.c_void_p, C.c_char_p, C.c_void_p)


class WebResult(C.Structure):
    _fields_ = [("uri", C.c_char_p), ("cookies", C.POINTER(C.c_char_p)), ("headers", C.POINTER(C.c_char_p))]


def library(path):
    scope = os.add_dll_directory(str(path.parent))
    dll = C.CDLL(str(path))
    signatures = {
        "openconnect_vpninfo_new": ([C.c_char_p, CERT_CB, C.c_void_p, FORM_CB, LOG_CB, C.c_void_p], C.c_void_p),
        "openconnect_vpninfo_free": ([C.c_void_p], None),
        "openconnect_set_protocol": ([C.c_void_p, C.c_char_p], C.c_int),
        "openconnect_parse_url": ([C.c_void_p, C.c_char_p], C.c_int),
        "openconnect_get_dnsname": ([C.c_void_p], C.c_char_p),
        "openconnect_get_hostname": ([C.c_void_p], C.c_char_p),
        "openconnect_get_port": ([C.c_void_p], C.c_int),
        "openconnect_set_cafile": ([C.c_void_p, C.c_char_p], C.c_int),
        "openconnect_set_webview_callback": ([C.c_void_p, BROWSER_CB], None),
        "openconnect_webview_load_changed": ([C.c_void_p, C.POINTER(WebResult)], C.c_int),
        "openconnect_obtain_cookie": ([C.c_void_p], C.c_int),
    }
    for name, (args, result) in signatures.items():
        function = getattr(dll, name)
        function.argtypes, function.restype = args, result
    dll.openconnect_init_ssl()
    return dll, scope


def run_case(dll, cert, key, mode, token_name, trusted=True, complete=True, probe=None, engine=None, transition=False, same_origin=False, saved_realm=False, failure="", dns_name=False):
    state = {"forms": 0, "browser": 0, "certificateRejected": 0, "submitted": [], "paths": [], "callbackResult": None}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            size = int(self.headers.get("Content-Length", "0"))
            body = self.rfile.read(size)
            endpoint = self.path.split("?")[0]
            status = 200
            if endpoint in ("/global-protect/prelogin.esp", "/ssl-vpn/prelogin.esp"):
                target = gateway_origin if transition and endpoint == "/ssl-vpn/prelogin.esp" else origin
                url = base64.b64encode((target + "/idp").encode()).decode()
                output = ("<prelogin-response><status>Success</status><saml-auth-method>REDIRECT</saml-auth-method>"
                          "<saml-request>" + url + "</saml-request></prelogin-response>").encode()
            else:
                state["submitted"].append(parse_qs(body.decode()))
                state["paths"].append(endpoint)
                if transition and endpoint == "/global-protect/getconfig.esp":
                    output = ("<policy><portal-name>Disposable portal</portal-name><gateways><external><list><entry name=\"" + gateway_origin[8:] + "\"><description>Fixture gateway</description></entry></list></external></gateways></policy>").encode()
                elif transition and state["submitted"][-1].get("portal-userauthcookie"):
                    # The library first tries the portal form; this gateway requires its own SSO.
                    status = 512
                    output = b""
                elif probe:
                    args = ["", "fixture-authcookie", "fixture-persistent", "fixture-portal", "fixture-user", "SAML", "vsys1", "fixture-domain", "", "", "", "", "tunnel", "-1", "4100", "10.0.0.2"]
                    output = ("<jnlp><application-desc>" + "".join("<argument>" + a + "</argument>" for a in args) + "</application-desc></jnlp>").encode()
                else:
                    # Stop after observing the form; no tunnel endpoint exists.
                    output = b'<response status="error"><error>fixture_stop_after_auth_form</error></response>'
            self.send_response(status)
            self.send_header("Content-Type", "application/xml")
            self.send_header("Content-Length", str(len(output)))
            self.send_header("Connection", "close")
            self.end_headers()
            self.wfile.write(output)

    if dns_name:
        assert all(ipaddress.ip_address(item[4][0]).is_loopback for item in socket.getaddrinfo("localhost", 0, type=socket.SOCK_STREAM)), "fixture_dns_must_remain_loopback"
    host = "localhost" if dns_name else "127.0.0.1"
    server = HTTPServer(("127.0.0.1", 0), Handler)
    origin = "https://" + host + ":" + str(server.server_port)
    gateway_server = HTTPServer(("127.0.0.1", 0), Handler) if transition and not same_origin else None
    gateway_origin = "https://" + host + ":" + str(gateway_server.server_port) if gateway_server else origin
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(cert, key)
    server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    gateway_thread = None
    if gateway_server:
        gateway_server.socket = context.wrap_socket(gateway_server.socket, server_side=True)
        gateway_thread = threading.Thread(target=gateway_server.serve_forever, daemon=True)
        gateway_thread.start()

    @CERT_CB
    def reject_cert(_context, _reason):
        state["certificateRejected"] += 1
        return -1

    @FORM_CB
    def form(_context, _form):
        state["forms"] += 1
        return 0

    @LOG_CB
    def quiet(_context, _level, _format):
        pass

    @BROWSER_CB
    def browser(vpn, uri, _context):
        state["browser"] += 1
        if dns_name:
            state["dnsDiffersFromPeer"] = dll.openconnect_get_dnsname(vpn).lower() == b"localhost" and dll.openconnect_get_hostname(vpn) == b"127.0.0.1"
            if not state["dnsDiffersFromPeer"]:
                return -1
        if uri.decode() != origin + "/idp":
            return -1
        pairs = [b"saml-username", b"fixture-user"]
        if complete:
            pairs += [token_name.encode(), b"fixture-token"]
        headers = (C.c_char_p * (len(pairs) + 1))(*pairs, None)
        value = WebResult((origin + "/SAML20/SP/ACS").encode(), None, headers)
        state["callbackResult"] = dll.openconnect_webview_load_changed(vpn, C.byref(value))
        return state["callbackResult"]

    vpn = None
    try:
        if probe:
            record = {"server": origin + ("/portal" if transition else "/gateway"), "caFile": str(cert), "tokenName": token_name}
            if transition:
                record.update(gateway=gateway_origin + "/gateway", realm="Fixture gateway" if saved_realm else "", failure=failure)
            process = subprocess.run([str(probe), str(engine)], input=(json.dumps(record) + "\n").encode(), stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=40)
            assert process.returncode == 0, "same_engine_auth_only_failed: " + process.stderr.decode("utf-8", "replace")
            result = json.loads(process.stdout)
            expected = {"authenticated": not failure, "challenges": 3 if transition else 1, "networkEvents": 0, "tunnelCreated": False}
            if failure:
                expected["failure"] = failure
            assert result == expected, "unexpected_engine_result"
        else:
            vpn = dll.openconnect_vpninfo_new(b"TermTerm-SSO-fixture/1", reject_cert, None, form, quiet, None)
            assert vpn
            assert dll.openconnect_set_protocol(vpn, b"gp") == 0
            assert dll.openconnect_parse_url(vpn, (origin + "/" + mode).encode()) == 0
            if trusted:
                assert dll.openconnect_set_cafile(vpn, str(cert).encode()) == 0
            dll.openconnect_set_webview_callback(vpn, browser)
            assert dll.openconnect_obtain_cookie(vpn) != 0
    finally:
        if vpn:
            dll.openconnect_vpninfo_free(vpn)
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
        if gateway_server:
            gateway_server.shutdown()
            gateway_server.server_close()
            gateway_thread.join(timeout=2)
    if transition:
        assert state["paths"] == ["/global-protect/getconfig.esp", "/ssl-vpn/login.esp"] + ([] if failure else ["/ssl-vpn/login.esp"]), "unexpected_transition_forms"
        assert state["submitted"][0].get("portal-userauthcookie") == ["fixture-token"], "portal_token_field_lost"
        if not failure:
            assert state["submitted"][-1].get("prelogin-cookie") == ["fixture-token"] and "passwd" not in state["submitted"][-1], "gateway_token_field_lost"
        return
    if not trusted:
        assert state["certificateRejected"] == 1 and state["browser"] == 0 and not state["submitted"], "untrusted_tls_reached_sso"
    elif not complete:
        assert state["callbackResult"] != 0 and not state["submitted"], "incomplete_headers_submitted"
    else:
        if not probe:
            assert state["forms"] == 1 and state["browser"] == 1 and state["callbackResult"] == 0, "sso_callback_not_reached"
            if dns_name:
                assert state["dnsDiffersFromPeer"], "dns_and_numeric_peer_not_distinguished"
        assert len(state["submitted"]) == 1, "auth_form_count"
        posted = state["submitted"][0]
        assert posted.get("user") == ["fixture-user"], "sso_username_lost"
        assert posted.get(token_name) == ["fixture-token"] and "passwd" not in posted, "sso_token_submitted_under_wrong_field"


def lab_contract():
    spec = importlib.util.spec_from_file_location("lab_broker", ROOT / "scripts/vpn-lab-broker.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)

    def fixture():
        writes = []
        session = SimpleNamespace(challenge={"id": 1, "kind": "sso", "fields": [], "private": {"expectedOrigin": "https://vpn.example"}},
                                  challenge_at=time.monotonic(), emit=lambda *_: None, write=writes.append)
        return session, writes

    valid = {"challengeId": 1, "result": {"uri": "https://vpn.example/SAML20/SP/ACS", "username": "fixture", "token": "fixture-token",
                                        "tokenName": "prelogin-cookie", "status": 200, "authStatus": "1"}}
    session, writes = fixture()
    module.Session.sso_answer(session, valid)
    assert session.challenge is None and writes[0]["action"] == "ssoAnswer"
    for key, value in [("uri", "https://idp.example/SAML20/SP/ACS"), ("tokenName", "passwd"), ("authStatus", "0"), ("token", "bad\nheader")]:
        session, writes = fixture()
        try:
            module.Session.sso_answer(session, {**valid, "result": {**valid["result"], key: value}})
            raise AssertionError("unsafe_result_accepted")
        except ValueError:
            assert not writes
    session, writes = fixture()
    session.challenge_at -= 301
    try:
        module.Session.sso_answer(session, valid)
        raise AssertionError("expired_result_accepted")
    except ValueError:
        assert not writes

    for choices, value, accepted in [([{"value": "gateway.example:444"}], "gateway.example:444", True), ([{"value": "gateway.example:444"}], "other.example", False), ([], "other.example", False)]:
        writes = []
        session = SimpleNamespace(profile={"family": "globalProtect"}, sso_origin="https://portal.example",
                                  challenge={"id": 2, "fields": [{"name": "gateway", "purpose": "choice", "choices": choices}]}, emit=lambda *_: None, write=writes.append)
        try:
            module.Session.answer(session, {"challengeId": 2, "values": {"gateway": value}})
            assert accepted and session.sso_origin == "https://gateway.example:444" and len(writes) == 1
        except ValueError:
            assert not accepted and not writes and session.sso_origin == "https://portal.example"
    session, writes = fixture()
    try:
        module.Session.answer(session, {"challengeId": 1, "values": {}})
        raise AssertionError("sso_form_answer_accepted")
    except ValueError:
        assert not writes


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dll", type=Path, default=TOOLS / "engine/libopenconnect-5.dll")
    parser.add_argument("--engine-probe", type=Path, help="Built debug termterm-vpn-auth-only-probe fixture driver")
    parser.add_argument("--engine", type=Path, help="Separate rebuilt engine, with its audited DLLs alongside")
    args = parser.parse_args()
    lab_contract()
    assert os.name == "nt", "native_windows_required"
    dll, scope = library(args.dll.resolve())
    openssl = Path(os.environ["ProgramFiles"]) / "Git/usr/bin/openssl.exe"
    with tempfile.TemporaryDirectory(prefix="termterm-gp-sso-") as temp:
        cert, key = Path(temp) / "ca.pem", Path(temp) / "key.pem"
        subprocess.run([str(openssl), "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                        "-subj", "/CN=TermTerm disposable SSO fixture", "-addext", "subjectAltName=IP:127.0.0.1,DNS:localhost",
                        "-keyout", str(key), "-out", str(cert)], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        count = 0
        for mode in ["portal", "gateway"]:
            for name in ["prelogin-cookie", "portal-userauthcookie"]:
                run_case(dll, cert, key, mode, name)
                count += 1
        run_case(dll, cert, key, "gateway", "prelogin-cookie", trusted=False)
        run_case(dll, cert, key, "gateway", "prelogin-cookie", complete=False)
        count += 2
        run_case(dll, cert, key, "gateway", "prelogin-cookie", dns_name=True)
        count += 1
        if args.engine_probe:
            engine = (args.engine or args.dll.resolve().with_name("termterm-openconnect.exe")).resolve()
            for name in ["prelogin-cookie", "portal-userauthcookie"]:
                run_case(dll, cert, key, "gateway", name, probe=args.engine_probe.resolve(), engine=engine)
                count += 1
            for options in [{}, {"same_origin": True}, {"saved_realm": True}, {"failure": "stale"}, {"failure": "offOrigin"}, {"failure": "cancel"}]:
                run_case(dll, cert, key, "portal", "portal-userauthcookie", probe=args.engine_probe.resolve(), engine=engine, transition=True, **options)
                count += 1
            for transition in [False, True]:
                run_case(dll, cert, key, "portal" if transition else "gateway", "prelogin-cookie", probe=args.engine_probe.resolve(), engine=engine, transition=transition, dns_name=True)
                count += 1
    scope.close()
    print(json.dumps({"samePublicApi": True, "localTlsCases": count, "sameEngineAuthOnlyCases": 10 if args.engine_probe else 0,
                      "dnsHostnameCases": 3 if args.engine_probe else 1,
                      "labContractCases": 10, "passed": True, "tunnelCreated": False}))


if __name__ == "__main__":
    main()
