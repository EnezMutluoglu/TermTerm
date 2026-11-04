#!/usr/bin/env python3
"""Exercise the real Linux native SSO response hook in private network/mount/display namespaces."""
import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import ssl
import subprocess as sp
import sys
import tempfile
import threading
import time


def run(*args, **kwargs):
    return sp.run(list(map(str,args)),check=True,capture_output=True,**kwargs)


def snapshot():
    return {"routes4":run("ip","-j","route","show","table","all").stdout.decode(),
            "routes6":run("ip","-j","-6","route","show","table","all").stdout.decode(),
            "dnsSha256":hashlib.sha256(Path("/etc/resolv.conf").read_bytes()).hexdigest(),
            "hostsSha256":hashlib.sha256(Path("/etc/hosts").read_bytes()).hexdigest(),
            "trustBundleSha256":hashlib.sha256(Path("/etc/ssl/certs/ca-certificates.crt").read_bytes()).hexdigest()}


def inside(stage):
    assert os.stat("/proc/self/ns/net").st_ino != os.stat("/proc/1/ns/net").st_ino
    assert os.stat("/proc/self/ns/mnt").st_ino != os.stat("/proc/1/ns/mnt").st_ino
    run("mount","--make-rprivate","/")
    (stage/"tmp").mkdir(mode=0o1777)
    (stage/"tmp").chmod(0o1777)
    run("mount","--bind",stage/"tmp","/tmp")
    Path("/tmp/.X11-unix").mkdir(mode=0o1777)
    Path("/tmp/.X11-unix").chmod(0o1777)
    run("mount","--bind",stage/"hosts","/etc/hosts")
    run("mount","--bind",stage/"trust-bundle.pem","/etc/ssl/certs/ca-certificates.crt")
    run("ip","link","set","lo","up")
    run("ip","addr","add","198.18.77.1/32","dev","lo")
    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*_): pass
        def do_GET(self):
            host=self.headers.get("Host","").split(":")[0]
            self.send_response(403 if host.startswith("rejected.") else 200)
            self.send_header("Content-Type","text/html")
            self.send_header("saml-username","fixture-user")
            self.send_header("prelogin-cookie","disposable-fixture-cookie")
            if host.startswith("duplicate."):
                self.send_header("prelogin-cookie","second-disposable-cookie")
            self.send_header("saml-auth-status","1")
            self.end_headers()
            self.wfile.write(b"<!doctype html><title>Disposable native SSO fixture</title>")
    server=HTTPServer(("198.18.77.1",31647),Handler)
    context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(stage/"certificate.pem",stage/"key.pem")
    server.socket=context.wrap_socket(server.socket,server_side=True)
    thread=threading.Thread(target=server.serve_forever,daemon=True)
    thread.start()
    display=None
    for _ in range(20):
        number=secrets.randbelow(1500)+200
        if not Path(f"/tmp/.X11-unix/X{number}").exists():
            display=number;break
    assert display is not None
    xlog=(stage/"xvfb.log").open("wb")
    xvfb=sp.Popen(["Xvfb",f":{display}","-screen","0","1280x900x24","-nolisten","tcp","-ac"],stdout=xlog,stderr=xlog)
    try:
        deadline=time.monotonic()+5
        while not Path(f"/tmp/.X11-unix/X{display}").exists():
            assert xvfb.poll() is None and time.monotonic()<deadline,"Xvfb startup failed: "+(stage/"xvfb.log").read_text()
            time.sleep(.02)
        environment=dict(os.environ,DISPLAY=f":{display}",TERMTERM_VPN_WEBKIT_FIXTURE=str(stage/"fixture.json"),
                         XDG_RUNTIME_DIR=str(stage/"runtime"),XDG_CACHE_HOME=str(stage/"cache"),GSETTINGS_BACKEND="memory")
        result=sp.run(["setpriv","--reuid","65534","--regid","65534","--clear-groups","dbus-run-session","--",str(stage/"test")],env=environment,capture_output=True,timeout=100)
        (stage/"result.json").write_text(json.dumps({"exitCode":result.returncode,"stdout":result.stdout.decode(errors="replace"),"stderr":result.stderr.decode(errors="replace")}))
        assert result.returncode==0,result.stdout.decode(errors="replace")+result.stderr.decode(errors="replace")
        for case in ["valid","wrong","rejected","duplicate","stale"]:
            assert f"VPN WebKit fixture {case}: passed" in result.stdout.decode(),f"Native fixture did not complete: {case}"
    finally:
        server.shutdown();server.server_close()
        xvfb.terminate();xvfb.wait(5);xlog.close()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test-executable",type=Path)
    parser.add_argument("--report",type=Path)
    parser.add_argument("--inside",type=Path)
    args=parser.parse_args()
    assert os.geteuid()==0,"Run only in the disposable root Linux lab"
    if args.inside:
        inside(args.inside);return
    assert args.test_executable and args.report
    before=snapshot()
    namespace="ttwebkit-"+secrets.token_hex(5)
    passed=False
    with tempfile.TemporaryDirectory(prefix="termterm-webkit-",dir="/run") as temporary:
        stage=Path(temporary);stage.chmod(0o755)
        shutil.copyfile(args.test_executable,stage/"test");(stage/"test").chmod(0o755)
        for directory in [stage/"runtime",stage/"cache"]:
            directory.mkdir(mode=0o700);os.chown(directory,65534,65534)
        hosts=Path("/etc/hosts").read_text()+"\n198.18.77.1 "+" ".join(f"{case}.sso.termterm.test" for case in ["valid","wrong","rejected","duplicate","stale"])+"\n"
        (stage/"hosts").write_text(hosts);(stage/"hosts").chmod(0o644)
        run("openssl","req","-x509","-newkey","rsa:2048","-nodes","-days","1","-subj","/CN=TermTerm disposable fixture CA",
            "-addext","basicConstraints=critical,CA:TRUE","-addext","keyUsage=critical,keyCertSign,cRLSign",
            "-keyout",stage/"ca-key.pem","-out",stage/"ca.pem")
        run("openssl","req","-new","-newkey","rsa:2048","-nodes","-subj","/CN=valid.sso.termterm.test",
            "-keyout",stage/"key.pem","-out",stage/"request.pem")
        (stage/"extensions.cnf").write_text("basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName="+
            ",".join("DNS:"+case+".sso.termterm.test" for case in ["valid","wrong","rejected","duplicate","stale"])+"\n")
        run("openssl","x509","-req","-in",stage/"request.pem","-CA",stage/"ca.pem","-CAkey",stage/"ca-key.pem","-CAcreateserial",
            "-days","1","-extfile",stage/"extensions.cnf","-out",stage/"certificate.pem")
        (stage/"trust-bundle.pem").write_bytes(Path("/etc/ssl/certs/ca-certificates.crt").read_bytes()+b"\n"+(stage/"ca.pem").read_bytes())
        (stage/"trust-bundle.pem").chmod(0o644);(stage/"ca-key.pem").chmod(0o600)
        (stage/"key.pem").chmod(0o600);(stage/"certificate.pem").chmod(0o644)
        (stage/"fixture.json").write_text(json.dumps({"port":31647}));(stage/"fixture.json").chmod(0o644)
        try:
            run("ip","netns","add",namespace)
            result=sp.run(["ip","netns","exec",namespace,"unshare","--mount","--",sys.executable,__file__,"--inside",str(stage)],capture_output=True,timeout=120)
            if result.returncode:
                raise AssertionError(result.stderr.decode(errors="replace")[-6000:])
            passed=True
        finally:
            run("ip","netns","delete",namespace)
            after=snapshot()
            report={"version":1,"passed":passed,"cases":["ephemeral WebView and its own private data manager","valid native ACS headers","wrong HTTPS origin ignored","non-2xx ACS rejected","duplicate token header rejected","stale native response blocked"],
                    "hostNetworkAndHostsUnchanged":before==after,"nativeTestExecutableSha256":hashlib.sha256(args.test_executable.read_bytes()).hexdigest(),
                    "companyGlobalProtectTested":False,"microsoftIdentityProviderTested":False,"macosTested":False,"desktopWindowTested":False,
                    "fixtureScope":"Real ephemeral WebKitGTK view, response signal and shared native header validation; disposable CA added only to a private mount-namespace system trust bundle.",
                    "result":json.loads((stage/"result.json").read_text()) if (stage/"result.json").exists() else None}
            args.report.parent.mkdir(parents=True,exist_ok=True);args.report.write_text(json.dumps(report,indent=2)+"\n")
            assert report["hostNetworkAndHostsUnchanged"]


if __name__=="__main__":main()
