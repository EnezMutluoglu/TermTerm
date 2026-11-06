#!/usr/bin/env python3
"""Opt-in vendor auth diagnostics; never create a tunnel or print server payloads."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import parse_qs, urlsplit

ROOT = Path(__file__).resolve().parents[1]
CLI = ROOT / ".tools/vpn-windows/openconnect-9.21-x64/openconnect.exe"
CLI_SHA256 = "cab1d965e403e0b435e90506d358b6345894da0360adcd7e846e27a41f870d96"
DLL_SHA256 = "370496877241a35f6883c316203ca8e1f0d02e31994d6315aef8138317e5d6c1"
AUTH_CLI = ROOT / ".tools/vpn-windows/auth-probe-v2/openconnect-auth-probe.exe"
AUTH_SHA256 = "24137da87d99497871f0a0c2e6d5abf470bb65e09e1f2853f53903e9ab4685e8"
AUTH_DLL_SHA256 = "59d60129ea53ca903448a17d61a25034a38849ceb64c13d2733be1b647f363bf"
TARGETS = ("gp", "fortinet")


def valid_server(raw):
    value = urlsplit(raw)
    if (value.scheme != "https" or not value.hostname or value.username or value.password
            or value.query or value.fragment or any(ord(c) < 33 or ord(c) > 126 for c in raw)):
        raise ValueError("https_server_without_credentials_required")
    if value.port is not None and not 1 <= value.port <= 65535:
        raise ValueError("invalid_port")
    return raw


def command(target, server, credentials=False, verbose=False):
    args = [str(AUTH_CLI if credentials else CLI), "--authenticate", "--non-inter", "--no-proxy",
            "--no-http-keepalive", "--protocol=" + target]
    if credentials:
        if target != "fortinet":
            raise ValueError("credentials_only_allowed_for_fortinet")
        args.append("--passwd-on-stdin")
    if verbose:
        if credentials:
            raise ValueError("verbose_only_allowed_without_credentials")
        args.append("--verbose")
    return args + [valid_server(server)]


def verified_cli(credentials=False):
    cli, pin, dll_pin = (AUTH_CLI, AUTH_SHA256, AUTH_DLL_SHA256) if credentials else (CLI, CLI_SHA256, DLL_SHA256)
    for path, expected in [(cli, pin), (cli.with_name("libopenconnect-5.dll"), dll_pin)]:
        if hashlib.sha256(path.read_bytes()).hexdigest() != expected:
            raise ValueError("unreviewed_binary")


def endpoint(raw):
    """Keep only the origin and known endpoint paths; queries may contain SAML data."""
    value = urlsplit(raw)
    if value.scheme != "https" or value.username or value.password or not value.hostname:
        return {"review": "non_https_or_embedded_sso_payload"}
    result = {"scheme": value.scheme, "host": value.hostname, "port": value.port or 443}
    if re.fullmatch(r"/(?:[0-9a-fA-F-]{36}|common|organizations)/(?:saml2|oauth2/(?:v2\.0/)?authorize)", value.path):
        result["path"] = value.path
    else:
        result["pathOmitted"] = True
    result["queryFields"] = sorted(set(parse_qs(value.query)) & {
        "SAMLRequest", "RelayState", "SAMLResponse", "SigAlg", "Signature", "login_hint"})
    return result


def summary(target, code, output, timed_out=False, credentials=False):
    # No raw line, cookie, token, username, or arbitrary server label reaches output.
    text = output.decode("utf-8", "replace")
    lower = text.lower()
    saml = re.search(r"SAML (REDIRECT|POST) authentication is required via ([^\r\n]+)", text)
    saml_metadata = re.search(r"<saml-auth-method>\s*(REDIRECT|POST)\s*</saml-auth-method>", text)
    result = {"target": target, "outcome": "stopped_on_error", "exitCode": code,
              "authenticationOnly": True, "credentialsProvided": credentials,
              "cliSha256": AUTH_SHA256 if credentials else CLI_SHA256,
              "dllSha256": AUTH_DLL_SHA256 if credentials else DLL_SHA256}
    result["tlsNegotiated"] = "Connected to HTTPS on " in text
    result["httpStatuses"] = [int(value) for value in re.findall(r"Got HTTP response: HTTP/1\.[01] ([1-5][0-9]{2})\b", text)][:10]
    if timed_out:
        result["outcome"] = "timeout_process_killed"
    elif credentials and code in {42, 43, 44, 45}:
        result["outcome"] = {42: "otp_challenge_reached_stopped", 43: "origin_changed_rejected",
                             44: "additional_auth_form_stopped", 45: "unexpected_form_rejected"}[code]
    elif "failed verification" in lower or "certificate verify failed" in lower:
        result["outcome"] = "tls_verification_failed"
    elif "getaddrinfo failed" in lower:
        result["outcome"] = "dns_resolution_failed"
    elif "failed to connect to host" in lower:
        result["outcome"] = "connection_failed"
    elif "ssl connection failure" in lower or "failed to establish ssl" in lower:
        result["outcome"] = "tls_handshake_failed"
    elif "unknown vpn protocol" in lower:
        result["outcome"] = "protocol_unsupported"
    elif (status := re.search(r"Unexpected ([1-5][0-9]{2}) result from server", text)):
        result["outcome"] = "http_rejected"
        result["httpStatus"] = int(status[1])
    elif "valid client certificate is required" in lower:
        result["outcome"] = "client_certificate_required"
    elif "error reading http response" in lower:
        result["outcome"] = "http_response_read_failed"
    elif "empty response" in lower:
        result["outcome"] = "http_response_empty"
    elif "failed to parse http response" in lower:
        result["outcome"] = "http_response_unrecognized"
    elif saml:
        # CLI without a webview deliberately returns EINVAL, then logs an XML error.
        result["outcome"] = "sso_required"
        result["ssoMethod"] = saml[1]
        result["ssoEndpoint"] = endpoint(saml[2].strip())
    elif (target == "gp" and saml_metadata and "<prelogin-response>" in text
          and re.search(r"<status>\s*Success\s*</status>", text)):
        # Secure CLI logs omit the SAML request; metadata alone cannot provide its URL.
        result["outcome"] = "sso_metadata_observed_browser_handoff_unavailable"
        result["ssoMethod"] = saml_metadata[1]
        result["responseFormat"] = "globalprotect_prelogin_xml"
    elif "failed to parse xml server response" in lower or "failed to parse non-xml server response" in lower:
        result["outcome"] = "protocol_response_unrecognized"
    elif "server is neither a globalprotect portal nor a gateway" in lower:
        result["outcome"] = "protocol_response_unrecognized"
    else:
        if code == 0:
            result["outcome"] = "authenticated_cookie_discarded"
        elif "User input required in non-interactive mode" in text:
            result["outcome"] = "credential_form_reached_no_submission"
    return result


def credential_input(raw):
    values = json.loads(raw)
    if not isinstance(values, dict) or set(values) != {"username", "password"}:
        raise ValueError("username_password_required")
    for value in values.values():
        if not isinstance(value, str) or not 1 <= len(value) <= 4094 or any(ord(c) < 32 or ord(c) > 126 for c in value):
            raise ValueError("single_line_ascii_credentials_required")
    return bytearray((values["password"] + "\n" + values["username"] + "\n").encode("ascii"))


def discover(target, server, payload=None, verbose=False):
    credentials = payload is not None
    process = subprocess.Popen(command(target, server, credentials, verbose), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, creationflags=subprocess.CREATE_NO_WINDOW,
                               env={**os.environ, "LC_ALL": "C", "LANG": "C", "LANGUAGE": "C"})
    timed_out = False
    try:
        output, _ = process.communicate(payload if credentials else b"", timeout=45)
    except subprocess.TimeoutExpired:
        timed_out = True
        process.kill()
        output, _ = process.communicate()
    finally:
        if payload is not None:
            payload[:] = b"\0" * len(payload)
        if process.poll() is None:
            process.kill()
            process.wait()
    return summary(target, process.returncode, output, timed_out, credentials)


def self_test():
    secret = "SYNTHETIC-COOKIE-AND-REQUEST"
    saml = ("SAML REDIRECT authentication is required via https://login.microsoftonline.com/"
            "11111111-2222-3333-4444-555555555555/saml2?SAMLRequest=" + secret + "&RelayState=" + secret)
    value = summary("gp", 1, saml.encode())
    assert value["outcome"] == "sso_required"
    assert value["ssoEndpoint"]["host"] == "login.microsoftonline.com"
    assert value["ssoEndpoint"]["queryFields"] == ["RelayState", "SAMLRequest"]
    assert secret not in json.dumps(value)
    value = summary("gp", 1, (saml + "\nFailed to parse XML server response\n").encode())
    assert value["outcome"] == "sso_required" and secret not in json.dumps(value)
    value = summary("gp", 1, b"<prelogin-response><status>Success</status><saml-auth-method>REDIRECT</saml-auth-method></prelogin-response>\nFailed to parse XML server response")
    assert value["outcome"] == "sso_metadata_observed_browser_handoff_unavailable"
    assert summary("gp", 0, ("COOKIE='" + secret + "'").encode())["outcome"] == "authenticated_cookie_discarded"
    assert secret not in json.dumps(summary("gp", 0, secret.encode()))
    assert summary("gp", 1, b"Certificate from VPN server failed verification.")["outcome"] == "tls_verification_failed"
    assert summary("gp", -9, b"", True)["outcome"] == "timeout_process_killed"
    assert summary("gp", 1, b"User input required in non-interactive mode")["outcome"] == "credential_form_reached_no_submission"
    for message, expected in [("getaddrinfo failed for host", "dns_resolution_failed"),
                              ("Failed to connect to host", "connection_failed"),
                              ("SSL connection failure", "tls_handshake_failed"),
                              ("Unknown VPN protocol", "protocol_unsupported"),
                              ("Unexpected 403 result from server", "http_rejected"),
                              ("Failed to parse XML server response", "protocol_response_unrecognized"),
                              ("Error reading HTTP response", "http_response_read_failed"),
                              ("Valid client certificate is required", "client_certificate_required")]:
        assert summary("gp", 1, message.encode())["outcome"] == expected
    assert endpoint("data:text/html;base64," + secret)["review"] == "non_https_or_embedded_sso_payload"
    assert endpoint("https://idp.example/" + secret)["pathOmitted"]
    args = command("gp", "https://vpn.example/portal")
    assert "--authenticate" in args and "--non-inter" in args
    assert "--no-http-keepalive" in args
    for invalid in ["http://vpn.example", "https://user:secret@vpn.example", "https://vpn.example/?secret=x", "https://vpn.example/\n"]:
        try:
            valid_server(invalid)
            raise AssertionError("unsafe_url_accepted")
        except ValueError:
            pass
    assert credential_input(b'{"username":"user","password":"pass"}') == b"pass\nuser\n"
    for invalid in [{"username": "user", "password": "bad\nOTP"}, {"username": "user\0", "password": "p"},
                    {"username": "user", "password": "p\r"}, {"username": "user", "password": "\u00e9"},
                    {"username": "user", "password": "p" * 4095}, {"username": "user"}]:
        try:
            credential_input(json.dumps(invalid).encode())
            raise AssertionError("unsafe_credential_accepted")
        except ValueError:
            pass
    assert summary("fortinet", 1, b"TERMTERM_AUTH_OTP")["outcome"] != "otp_challenge_reached_stopped"
    assert summary("fortinet", 42, b"", credentials=True)["outcome"] == "otp_challenge_reached_stopped"
    assert summary("fortinet", 43, b"", credentials=True)["outcome"] == "origin_changed_rejected"
    value = summary("gp", 1, b"Connected to HTTPS on synthetic\nGot HTTP response: HTTP/1.1 200 OK\nFailed to parse XML server response\n")
    assert value["tlsNegotiated"] and value["httpStatuses"] == [200]
    assert value["outcome"] == "protocol_response_unrecognized"
    assert "--verbose" in command("gp", "https://vpn.example/portal", verbose=True)
    print(json.dumps({"selfTest": "passed", "checks": 38}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", nargs="?", choices=TARGETS)
    parser.add_argument("--server", help="Approved HTTPS origin/path; no credentials or query")
    parser.add_argument("--credentials-stdin", action="store_true", help="Fortinet only: one JSON username/password record then EOF")
    parser.add_argument("--verbose", action="store_true", help="Secretless diagnostics only; raw output remains captured in memory")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    elif args.target and args.server:
        try:
            if os.name != "nt":
                raise ValueError("windows_required")
            command(args.target, args.server, args.credentials_stdin, args.verbose)
            verified_cli(args.credentials_stdin)
            payload = None
            if args.credentials_stdin:
                raw = sys.stdin.buffer.read(16385)
                if len(raw) > 16384:
                    raise ValueError("credential_record_too_large")
                payload = credential_input(raw)
                del raw
            print(json.dumps(discover(args.target, args.server, payload, args.verbose)))
        except Exception:
            print(json.dumps({"outcome": "local_probe_error_no_raw_details"}))
            raise SystemExit(1)
    else:
        parser.error("target and --server, or --self-test required")
