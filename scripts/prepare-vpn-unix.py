#!/usr/bin/env python3
"""Prepare development/candidate helper files; never install, start, publish or update anything."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import stat
import subprocess
from xml.sax.saxutils import escape

ROOT=Path(__file__).resolve().parents[1]
IDENTITY="local.termterm.desktop.vpndev.broker"
LINUX_UNIT="""[Unit]
Description=TermTerm isolated VPN development broker
After=network.target
[Service]
Type=simple
ExecStart=/usr/bin/python3 -I /opt/termterm-vpn-dev/bin/vpn-unix-broker.py
RuntimeDirectory=termterm-vpn-dev
RuntimeDirectoryMode=0755
StateDirectory=termterm-vpn-dev
StateDirectoryMode=0700
UMask=0077
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/run/termterm-vpn-dev /var/lib/termterm-vpn-dev
RestrictAddressFamilies=AF_UNIX AF_INET AF_NETLINK
# /proc peer-executable verification across UIDs requires CAP_SYS_PTRACE.
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW CAP_SETUID CAP_SETGID CAP_DAC_READ_SEARCH CAP_CHOWN CAP_SYS_PTRACE
TimeoutStopSec=45
KillMode=mixed
[Install]
WantedBy=multi-user.target
"""
MAC_PLIST="""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>local.termterm.desktop.vpndev.broker</string>
<key>ProgramArguments</key><array><string>__TERMTERM_VERIFIED_PYTHON__</string><string>-I</string><string>/Library/PrivilegedHelperTools/local.termterm.desktop.vpndev.broker/vpn-unix-broker.py</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
<key>Umask</key><integer>63</integer>
</dict></plist>
"""


def linux_unit(candidate=None):
    if candidate is None:
        return LINUX_UNIT
    if candidate != "0.4.4":
        raise ValueError("Unsupported production candidate version")
    return LINUX_UNIT.replace("isolated VPN development broker", "VPN broker").replace("termterm-vpn-dev", "termterm-vpn").replace("vpn-unix-broker.py\n", "vpn-unix-broker.py --candidate 0.4.4\n")


def mac_plist(candidate=None):
    if candidate is None:
        return MAC_PLIST
    if candidate != "0.4.4":
        raise ValueError("Unsupported production candidate version")
    return MAC_PLIST.replace("local.termterm.desktop.vpndev.broker", "local.termterm.desktop.broker").replace("vpn-unix-broker.py</string>", "vpn-unix-broker.py</string><string>--candidate</string><string>0.4.4</string>")


def mac_python(path):
    if not path.is_absolute() or path != path.resolve() or not path.is_file():
        raise ValueError("macOS Python must be a canonical absolute regular file")
    for part in [path, *path.parents]:
        metadata=part.lstat()
        if metadata.st_uid!=0 or metadata.st_mode & 0o022 or stat.S_ISLNK(metadata.st_mode):
            raise ValueError("macOS Python and every ancestor must be root owned and not group/world writable")
    probe=subprocess.run([str(path),"-I","-c","import json,sys; print(json.dumps({'version':list(sys.version_info[:2]),'executable':sys.executable}))"],check=True,capture_output=True,text=True,timeout=8)
    runtime=json.loads(probe.stdout)
    if tuple(runtime["version"]) < (3,10) or Path(runtime["executable"]).resolve()!=path:
        raise ValueError("macOS broker requires the selected Python 3.10 or newer runtime")
    return {"python":str(path),"pythonSha256":hashlib.sha256(path.read_bytes()).hexdigest()}


def source_files():
    spec=importlib.util.spec_from_file_location("vpn_public_source",ROOT/"scripts/collect-vpn-runtime.py")
    public=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(public)
    approved=set(public.source_paths())
    files=["scripts/vpn-unix-broker.py","scripts/vpn-lab-broker.py","scripts/prepare-vpn-unix.py","scripts/install-vpn-appimage.py","scripts/build-wireguard.py",
           "scripts/test-vpn-unix-service.py","scripts/test-vpn-lab.py","scripts/test-vpn-webkit.py",
            "scripts/vpn-dns-fixture.py","scripts/vpn_lab_json.py",
           "scripts/vpn-windows-dependencies.json","scripts/collect-vpn-runtime.py","scripts/vpn-public-source-files.json",
           "src-tauri/src/vpn_sso/unix.rs","src-tauri/src/vpn_sso/webkit.m","src-tauri/tests/vpn_webkit_fixture.rs",
           "docs/VPN_UNIX_KURULUM_TR.md","docs/VPN_WIREGUARD_TR.md"]
    # Keep nested patches and license notices: the root-level engine files alone cannot reproduce the build.
    files += [relative for relative in approved if relative.startswith(("vpn-engine/","vpn-core/src/","vpn-wireguard/","docs/licenses/vpn/"))]
    files += ["vpn-core/Cargo.toml","vpn-core/Cargo.lock"]
    for relative in files:
        path=ROOT/relative
        if relative not in approved:
            raise ValueError("Preparation source is absent from the reviewed public inventory")
        public.no_links(path)
    return sorted(set(files))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform",choices=["linux","macos"],required=True)
    parser.add_argument("--candidate",choices=["0.4.4"],help="Prepare the explicit production candidate contract; never install it")
    parser.add_argument("--client-install",choices=["deb","appimage"],default="deb",help="Linux candidate: fixed DEB or root-owned AppDir path")
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--source-only",action="store_true")
    parser.add_argument("--engines",type=Path)
    parser.add_argument("--client",type=Path,help="The separately built executable for the selected development or exact candidate contract")
    parser.add_argument("--uid",type=int)
    parser.add_argument("--client-requirement",help="macOS designated requirement from the signed isolated development app")
    parser.add_argument("--python-runtime",type=Path,help="macOS: canonical root-owned Python 3.10+ executable, including trusted runtime dependencies")
    args=parser.parse_args()
    if args.client_install == "appimage" and (args.platform != "linux" or not args.candidate):
        parser.error("AppImage pairing requires the explicit Linux production candidate")
    service_name = "termterm-vpn" if args.candidate else "termterm-vpn-dev"
    identity = "local.termterm.desktop.broker" if args.candidate else IDENTITY
    support = "TermTerm VPN" if args.candidate else "TermTerm VPN Dev"
    output=args.output.resolve()
    if not output.is_relative_to((ROOT/".tools").resolve()) or output.exists():
        parser.error("Choose a new directory inside this worktree's .tools; existing files are never replaced")
    if not args.source_only and (not args.engines or not args.client or not args.uid or args.uid<=0):
        parser.error("Binary preparation needs --engines, --client and a nonroot --uid")
    if args.platform=="macos" and not args.source_only and not args.client_requirement:
        parser.error("macOS requires the signed development client's designated requirement")
    runtime={}
    if args.platform=="macos" and not args.source_only:
        if not args.python_runtime:
            parser.error("macOS binary preparation requires --python-runtime; the system Python is not assumed")
        runtime=mac_python(args.python_runtime)
    output.mkdir(parents=True)
    hashes={}
    for relative in source_files():
        original=ROOT/relative
        destination=output/"source"/relative
        destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(original,destination)
        hashes[relative]=hashlib.sha256(original.read_bytes()).hexdigest()
    rootfs=output/"rootfs"
    if args.platform=="linux":
        helper=Path("opt")/service_name/"bin"
        configuration=Path("etc")/service_name/"broker.json"
        journal="/var/lib/"+service_name
        unit=rootfs/"etc/systemd/system"/(service_name+".service")
        unit.parent.mkdir(parents=True);unit.write_text(linux_unit(args.candidate))
    else:
        helper=Path("Library/PrivilegedHelperTools")/identity
        configuration=Path("Library/Application Support")/support/"broker.json"
        journal="/Library/Application Support/"+support+"/journal"
        for directory,mode in [(rootfs/"private/var/run"/service_name,0o755),(rootfs/configuration.parent,0o700),(rootfs/journal.lstrip("/"),0o700)]:
            directory.mkdir(parents=True,exist_ok=True);directory.chmod(mode)
        unit=rootfs/"Library/LaunchDaemons"/(identity+".plist")
        unit.parent.mkdir(parents=True);unit.write_text(mac_plist(args.candidate).replace("__TERMTERM_VERIFIED_PYTHON__",escape(runtime.get("python","/REQUIRES_VERIFIED_PYTHON_3_10_OR_NEWER/python3"))))
    binaries={}
    if not args.source_only:
        names=["termterm-openconnect","termterm-openvpn","termterm-wireguard","vpn-policy-lab"]+(["termterm-vpn-macos"] if args.platform=="macos" else [])
        for name in names:
            original=args.engines/name
            if not original.is_file() or original.is_symlink():
                parser.error("All compiled helpers must be regular nonsymlink files: "+name)
            destination=rootfs/helper/name;destination.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(original,destination);destination.chmod(0o755)
            binaries[name]=hashlib.sha256(destination.read_bytes()).hexdigest()
        for name in ["vpn-unix-broker.py","vpn-lab-broker.py"]:
            shutil.copyfile(ROOT/"scripts"/name,rootfs/helper/name)
        if args.platform == "linux" and args.candidate:
            shutil.copyfile(ROOT/"scripts/install-vpn-appimage.py",rootfs/helper/"install-vpn-appimage.py")
        # The client remains in its explicitly selected installed development bundle.
        # Root-owned executable, ancestor, signature and SHA checks run again at service startup.
        client=args.client.resolve()
        config={"uid":args.uid,"client":str(client),"clientSha256":hashlib.sha256(client.read_bytes()).hexdigest(),"engines":"/"+helper.as_posix(),"pins":binaries,"journal":journal}
        if args.candidate:
            config.update(applicationIdentifier="local.termterm.desktop",candidateVersion=args.candidate,
                          client="/usr/bin/termterm" if args.platform=="linux" else "/Applications/TermTerm.app/Contents/MacOS/termterm")
            if args.client_install == "appimage":
                config.update(client="/opt/termterm-appimage/usr/bin/termterm",clientInstall="appimage")
        if args.platform=="macos":config.update(clientRequirement=args.client_requirement,**runtime)
        destination=rootfs/configuration;destination.parent.mkdir(parents=True,exist_ok=True)
        destination.write_text(json.dumps(config,indent=2)+"\n");destination.chmod(0o600)
    manifest={"identity":identity,"candidateVersion":args.candidate,"platform":args.platform,"sourceOnly":args.source_only,"installed":False,"serviceStarted":False,"distributionReady":False,
              "desktopSourceSubset":True,"desktopBuildRequiresRepository":True,"externalDependenciesIncluded":False,
              "macosNativeTested":False,"sources":hashes,"binaries":binaries,
              "requiredBeforeInstallation":["root ownership and nonwritable ancestors of client, helpers and every dynamic dependency","exact 0.4.4 production candidate desktop" if args.candidate else "separate vpndev desktop identity","Python runtime at the service template's absolute path","native platform acceptance tests; source-only output cannot start a service"]}
    if args.platform=="linux":
        manifest["linuxRuntime"]={"referenceDistribution":"Ubuntu 24.04", "aptPackages":["python3","iproute2","systemd-resolved","binutils","libjsoncpp25","libopenconnect5","libssl3t64","liblz4-1","ca-certificates"],
                                  "otherDistributionsRequireEquivalentDependencies":True}
    else:
        manifest["macosRuntime"]={"pythonMinimum":"3.10","pythonConfigured":bool(runtime),**runtime,"frameworkAndStdlibBundled":False,
                                  "directories":{"/private/var/run/"+service_name:"root:0755","/Library/Application Support/"+support:"root:0700",journal:"root:0700"}}
        manifest["requiredBeforeInstallation"] += ["macOS: resolve the source-only Python placeholder before launch; verify the selected Python executable, standard library, frameworks and every dynamic dependency are root owned and not user writable"]
    (output/"PREPARATION.json").write_text(json.dumps(manifest,indent=2)+"\n")
    print("Prepared source/package files only; no installation: "+str(output))


if __name__=="__main__":main()
