#!/usr/bin/env python3
"""Check reviewed VPN development or exact production candidate packages in disposable WSL only."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import secrets
import shutil
import signal
import subprocess as sp
import sys
import tarfile
import tempfile
import time

DISTROS = ("TermTerm-VPN-Package-Acceptance-20261104", "TermTerm-VPN-Package-Acceptance-20261105-dev4")
MARKER = Path("/.termterm-vpn-package-acceptance")
ROOT = Path("/root/termterm-vpn-package-acceptance")
HOME = Path("/home/vpnaccept")
VERSION = r"(\d+\.\d+\.\d+)-dev\.(0|[1-9][0-9]{0,8})"


def require(value, message):
    if not value:
        raise ValueError(message)


def configure(candidate=None):
    require(candidate in (None, "0.4.4"), "Only the exact 0.4.4 candidate is supported")
    global CANDIDATE, IDENTITY, TITLE, NAME, SERVICE, PACKAGES, CONFIG, CLIENT, BROKER, UNIT, JOURNAL, SOCKET
    CANDIDATE = candidate
    IDENTITY = "local.termterm.desktop" if candidate else "local.termterm.desktop.vpndev"
    TITLE = "TermTerm" if candidate else "TermTerm VPN Dev"
    NAME = "termterm" if candidate else "termterm-vpn-dev"
    SERVICE = "termterm-vpn" if candidate else "termterm-vpn-dev"
    PACKAGES = ("term-term", "termterm-vpn-broker") if candidate else ("term-term-vpn-dev", "termterm-vpn-dev-broker")
    CONFIG = Path("/etc/" + SERVICE + "/broker.json")
    CLIENT = Path("/usr/bin/" + NAME)
    BROKER = Path("/opt/" + SERVICE + "/bin/vpn-unix-broker.py")
    UNIT = SERVICE + ".service"
    JOURNAL = Path("/var/lib/" + SERVICE)
    SOCKET = Path("/run/" + SERVICE + "/broker.sock")


configure()


def distro_matches(name, marker):
    return name in DISTROS and marker == name + "\n"


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def plain(path):
    path = path.absolute()
    require(path == path.resolve() and not any(p.is_symlink() for p in [path, *path.parents]), "Linked or noncanonical path refused")
    return path


def fresh(path, parent):
    plain(path)
    require(path.parent == plain(parent) and not path.exists(), "Use a fresh contained acceptance directory; preserve existing data")


def upgrade(previous, current):
    if CANDIDATE:
        require(previous is None and current == CANDIDATE, "Candidate checks cover clean installation only; dev-to-production and unreviewed stable upgrades are not accepted")
        return
    old, new = re.fullmatch(VERSION, previous), re.fullmatch(VERSION, current)
    require(old and new and old[1] == new[1] and int(new[2]) > int(old[2]), "Require increasing dev.N versions on the same base; reinstall is not upgrade coverage")


def receipt(directory, expected):
    directory = plain(directory)
    path = plain(directory / "PACKAGE-RECEIPT.json")
    require(re.fullmatch(r"[0-9a-f]{64}", expected) and digest(path) == expected, "Reviewed package receipt SHA256 differs")
    data = json.loads(path.read_text())
    require(data["identity"] == IDENTITY and data["developmentOnly"] is (not bool(CANDIDATE)) and type(data["brokerUid"]) is int
            and data["brokerUid"] == 1000 and (data["version"] == CANDIDATE if CANDIDATE else re.fullmatch(VERSION, data["version"])), "Only the selected UID1000 package identity/version is accepted")
    if CANDIDATE:
        require(data["candidateVersion"] == CANDIDATE and data["desktopPackage"] == PACKAGES[0] and data["brokerService"] == UNIT
                and data["distributionReady"] is False and data["releaseCandidate"] is True and data["updateChannel"] == "stable", "Final production candidate receipt required")
        require(set(data["brokerPins"]) == {"vpn-policy-lab", "termterm-openvpn", "termterm-openconnect", "termterm-wireguard"}
                and all(re.fullmatch(r"[0-9a-f]{64}", pin) for pin in data["brokerPins"].values()), "Exact candidate helper pins required")
    names = [f"{TITLE}_{data['version']}_amd64.deb", f"{PACKAGES[1]}_{data['version']}_amd64.deb"]
    if not CANDIDATE:
        names.append(f"{TITLE}_{data['version']}_amd64.AppImage")
    require(re.fullmatch(r"[0-9a-f]{64}", data["debClientSha256"]) is not None, "Missing installed desktop hash")
    for name in names:
        item = data["files"][name]
        path = plain(directory / name)
        require(path.is_file() and 0 < item["bytes"] <= 512 * 1024 * 1024 and path.stat().st_size == item["bytes"]
                and re.fullmatch(r"[0-9a-f]{64}", item["sha256"]) and digest(path) == item["sha256"], "Reviewed package bytes differ")
    return {"directory": directory, "receipt": data, "names": names}


def command(arguments, *, check=True, timeout=60, env=None, input=None):
    result = sp.run(list(map(str, arguments)), input=input, capture_output=True, text=True, timeout=timeout,
                    env=env or {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "LC_ALL": "C", "HOME": "/root", "DEBIAN_FRONTEND": "noninteractive"})
    require(not check or result.returncode == 0, "Acceptance command failed: " + Path(str(arguments[0])).name)
    return result


def members(raw, *, broker=False, control=False):
    files = {}
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive:
            name = member.name.removeprefix("./").rstrip("/")
            if name in {"", "."} and member.isdir():
                continue
            path = PurePosixPath(name)
            require(not path.is_absolute() and ".." not in path.parts and path.as_posix() == name and "\\" not in name
                    and member.uid == member.gid == 0 and not member.mode & 0o6000 and (member.isfile() or member.isdir()), "Unsafe DEB member")
            if member.isdir():
                roots = (["etc/" + SERVICE, "opt/" + SERVICE, "usr/lib/systemd/system", "usr/share/doc/" + PACKAGES[1]] if broker
                         else ["usr/bin", "usr/lib/" + TITLE, "usr/share/applications", "usr/share/icons/hicolor"])
                require(not control and any(name == root or name.startswith(root + "/") or root.startswith(name + "/") for root in roots), "DEB directory escapes its namespace")
                continue
            require(name not in files and member.size <= 128 * 1024 * 1024, "Duplicate or oversized DEB member")
            if control:
                require(name in {"control", "md5sums", "conffiles"}, "Maintainer scripts or unknown control files require review")
            elif broker:
                require(name == CONFIG.as_posix().lstrip("/") or name == "usr/lib/systemd/system/" + UNIT
                        or name.startswith(("opt/" + SERVICE + "/", "usr/share/doc/" + PACKAGES[1] + "/")), "Broker package escapes its namespace")
            else:
                require(name == CLIENT.as_posix().lstrip("/") or name == "usr/share/applications/" + TITLE + ".desktop"
                        or name.startswith("usr/lib/" + TITLE + "/")
                        or re.fullmatch(r"usr/share/icons/hicolor/[^/]+/apps/" + NAME + r"\.png", name), "Desktop package escapes its namespace")
            files[name] = archive.extractfile(member).read()
    return files


def validate_broker_config(parsed, data):
    require(type(parsed["uid"]) is int and parsed["uid"] == 1000 and parsed["client"] == CLIENT.as_posix() and parsed["clientSha256"] == data["debClientSha256"]
            and parsed["engines"] == BROKER.parent.as_posix() and parsed["pins"] == data["brokerPins"]
            and parsed["journal"] == JOURNAL.as_posix(), "Packaged broker pins differ from the reviewed receipt")
    if CANDIDATE:
        require(parsed["applicationIdentifier"] == IDENTITY and parsed["candidateVersion"] == CANDIDATE
                and "clientInstall" not in parsed, "Candidate DEB must use the fixed production identity and DEB client")


def validate_candidate_unit(raw):
    lines = raw.decode().splitlines()
    require([line for line in lines if line.startswith("Exec")] == ["ExecStart=/usr/bin/python3 -I " + BROKER.as_posix() + " --candidate " + CANDIDATE]
            and "RuntimeDirectory=" + SERVICE in lines and "StateDirectory=" + SERVICE in lines,
            "Candidate service must select the exact production broker/runtime/journal contract")


def inspect(package):
    data = package["receipt"]
    for index, name in enumerate(package["names"][:2]):
        path = package["directory"] / name
        fields = {}
        for field in ["Package", "Version", "Architecture", "Conflicts", "Replaces", "Breaks", "Depends"]:
            fields[field] = command(["dpkg-deb", "--field", path, field]).stdout.strip()
        require(fields["Package"] == PACKAGES[index] and fields["Version"] == data["version"] and fields["Architecture"] == "amd64"
                and not any(fields[key] for key in ["Conflicts", "Replaces", "Breaks"]), "DEB identity/version or stable compatibility boundary differs")
        if index and not CANDIDATE:
            require(f"{PACKAGES[0]} (= {data['version']})" in fields["Depends"], "Broker must require the matching development desktop")
        control = sp.check_output(["dpkg-deb", "--ctrl-tarfile", str(path)], timeout=30)
        controls = members(control, control=True)
        require(controls.get("conffiles", b"").strip() == (CONFIG.as_posix().encode() if index else b""), "Unexpected retained DEB configuration paths")
        files = members(sp.check_output(["dpkg-deb", "--fsys-tarfile", str(path)], timeout=30), broker=bool(index))
        if index:
            if CANDIDATE:
                validate_candidate_unit(files["usr/lib/systemd/system/" + UNIT])
            config = files[CONFIG.as_posix().lstrip("/")]
            parsed = json.loads(config)
            validate_broker_config(parsed, data)
            package["config"] = config
        else:
            require(hashlib.sha256(files[CLIENT.as_posix().lstrip("/")]).hexdigest() == data["debClientSha256"], "DEB desktop hash differs")


def empty_journal():
    path = JOURNAL / "owned.json"
    plain(path)
    require(not path.exists() or json.loads(path.read_text()) == {}, "Owned VPN sessions remain; preserve service/packages for inspection")


def stop_service():
    empty_journal()
    command(["systemctl", "stop", UNIT], timeout=55)
    require(command(["systemctl", "is-active", "--quiet", UNIT], check=False).returncode != 0, "Development broker did not stop")
    empty_journal()


def check_service():
    command(["python3", "-I", BROKER, "--check", *(["--candidate", CANDIDATE] if CANDIDATE else [])])
    command(["systemctl", "daemon-reload"])
    command(["systemctl", "start", UNIT])
    command(["systemctl", "is-active", "--quiet", UNIT])
    deadline = time.monotonic() + 5
    while not SOCKET.is_socket() and time.monotonic() < deadline:
        command(["systemctl", "is-active", "--quiet", UNIT])
        time.sleep(0.1)
    require(SOCKET.is_socket(), "Packaged DEFAULT broker did not expose its authenticated Unix socket")
    empty_journal()
    stop_service()


def group_alive(group):
    for path in Path("/proc").glob("[0-9]*/stat"):
        try:
            fields = path.read_text().rsplit(") ", 1)[1].split()
            if int(fields[2]) == group and fields[0] != "Z":
                return True
        except (FileNotFoundError, ProcessLookupError):
            pass
    return False


def terminate_owned(process):
    # The launcher can exit while its children retain our session/process group.
    if group_alive(process.pid):
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        deadline = time.monotonic() + 5
        while group_alive(process.pid) and time.monotonic() < deadline:
            time.sleep(0.1)
        if group_alive(process.pid):
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    process.wait(timeout=5)
    deadline = time.monotonic() + 5
    while group_alive(process.pid) and time.monotonic() < deadline:
        time.sleep(0.1)
    require(not group_alive(process.pid), "Owned UI descendant remains after cleanup")


def window(program, case, label):
    runtime = HOME / ".cache" / ("termterm-package-window-" + label + "-" + case.name)
    fresh(runtime, runtime.parent)
    runtime.mkdir(mode=0o700, parents=True)
    os.chown(runtime, 1000, 1000)
    authority = runtime / "Xauthority"
    authority.touch(mode=0o600)
    os.chown(authority, 1000, 1000)
    # A dedicated authenticated display avoids the existing WSLg desktop and its windows.
    require(not Path("/tmp/.X91-lock").exists() and not Path("/tmp/.X11-unix/X91").exists(), "Acceptance X display is already in use")
    command(["xauth", "-f", authority], input="add :91 MIT-MAGIC-COOKIE-1 " + secrets.token_hex(16) + "\n")
    os.chown(authority, 1000, 1000)
    if program != CLIENT:
        staged = runtime / "TermTermVPNDev.AppImage"
        shutil.copyfile(program, staged)
        require(digest(staged) == digest(program), "AppImage staging changed reviewed bytes")
        os.chown(staged, 1000, 1000)
        staged.chmod(0o500)
        program = staged
    env = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "LC_ALL": "C", "HOME": str(HOME), "USER": "vpnaccept", "LOGNAME": "vpnaccept",
           "DISPLAY": ":91", "XAUTHORITY": str(authority), "XDG_RUNTIME_DIR": str(runtime), "APPIMAGE_EXTRACT_AND_RUN": "1"}
    processes = []
    with (case / (label + "-window.log")).open("w") as log:
        try:
            xvfb = sp.Popen(["Xvfb", ":91", "-screen", "0", "1280x800x24", "-nolisten", "tcp", "-auth", str(authority)], stdout=log, stderr=log, start_new_session=True)
            processes.append(xvfb)
            deadline = time.monotonic() + 10
            while command(["xdpyinfo"], env=env, check=False).returncode != 0:
                require(xvfb.poll() is None and time.monotonic() < deadline, "Owned X server failed")
                time.sleep(0.2)
            user = ["setpriv", "--reuid=1000", "--regid=1000", "--init-groups", "env", "-i", *[key + "=" + value for key, value in env.items()]]
            wm = sp.Popen([*user, "openbox"], stdout=log, stderr=log, start_new_session=True)
            processes.append(wm)
            deadline = time.monotonic() + 10
            while command(["wmctrl", "-m"], env=env, check=False).returncode != 0:
                require(wm.poll() is None and time.monotonic() < deadline, "Owned window manager failed")
                time.sleep(0.1)
            app = sp.Popen([*user, "dbus-run-session", "--", str(program)], stdout=log, stderr=log, start_new_session=True)
            processes.append(app)
            deadline = time.monotonic() + 45
            owned_window = None
            while app.poll() is None and time.monotonic() < deadline:
                for line in command(["wmctrl", "-lp"], env=env).stdout.splitlines():
                    parts = line.split(None, 4)
                    if len(parts) >= 3 and parts[2].isdigit():
                        pid = int(parts[2])
                        try:
                            if os.getpgid(pid) == app.pid and Path(f"/proc/{pid}").stat().st_uid == 1000:
                                owned_window = parts[0]
                                break
                        except ProcessLookupError:
                            pass
                if owned_window:
                    break
                time.sleep(0.2)
            require(owned_window is not None, "Packaged development desktop did not render an owned window")
            command(["wmctrl", "-i", "-c", owned_window], env=env)
            require(app.wait(timeout=15) == 0, "Packaged desktop did not close gracefully")
            deadline = time.monotonic() + 5
            while group_alive(app.pid) and time.monotonic() < deadline:
                time.sleep(0.1)
            require(not group_alive(app.pid), "Desktop left an owned descendant after graceful close")
        finally:
            errors = []
            for process in reversed(processes):
                try:
                    terminate_owned(process)
                except Exception as error:
                    errors.append(type(error).__name__)
            require(not errors, "Owned window process cleanup failed: " + ", ".join(errors))


def installed(package):
    for name in PACKAGES:
        result = command(["dpkg-query", "-W", "-f=${db:Status-Status}\t${Version}", name]).stdout
        require(result == "installed\t" + package["receipt"]["version"], "Installed development package version differs")
    require(digest(CLIENT) == package["receipt"]["debClientSha256"], "Installed desktop bytes differ")


def stale_pin_rejected(result):
    require(result.returncode != 0 and result.stderr.rstrip().endswith("ValueError: executable_pin_mismatch"), "Expected the preserved stale executable pin to fail; unrelated broker errors are not acceptance evidence")


def run(args):
    require(not CANDIDATE or args.previous is None and args.expected_previous_receipt_sha256 is None, "Candidate mode is clean-install only; do not supply a development or unreviewed stable baseline")
    distro = os.environ.get("WSL_DISTRO_NAME")
    require(sys.platform == "linux" and os.geteuid() == 0 and distro in DISTROS
            and "microsoft" in Path("/proc/sys/kernel/osrelease").read_text().lower(), "Run only as root in the dedicated disposable WSL acceptance distro")
    plain(MARKER)
    require(MARKER.is_file() and MARKER.stat().st_uid == 0 and not MARKER.stat().st_mode & 0o022
            and distro_matches(distro, MARKER.read_text()), "A root-owned acceptance marker matching this distro is required")
    require(not CANDIDATE or distro == DISTROS[1], "Production candidate checks require the fresh candidate guest; preserve the original development guest")
    release = dict(line.split("=", 1) for line in Path("/etc/os-release").read_text().splitlines() if "=" in line)
    require(release["ID"].strip('"') == "ubuntu" and release["VERSION_ID"].strip('"') == "24.04" and Path("/proc/1/comm").read_text().strip() == "systemd", "Clean Ubuntu24.04 with systemd required")
    import pwd
    account = pwd.getpwnam("vpnaccept")
    require(account.pw_uid == account.pw_gid == 1000 and account.pw_dir == str(HOME) and plain(HOME).stat().st_uid == 1000, "Dedicated UID1000 vpnaccept home required")
    for tool in ["dpkg", "dpkg-deb", "dpkg-query", "systemctl", "setpriv", "Xvfb", "xdpyinfo", "xauth", "wmctrl", "openbox", "dbus-run-session"]:
        require(shutil.which(tool), "Install guest prerequisite before acceptance: " + tool)
    for name in ["term-term-vpn-dev", "termterm-vpn-dev-broker", "termterm", "term-term", "termterm-vpn-broker"]:
        result = command(["dpkg-query", "-W", "-f=${db:Status-Status}", name], check=False)
        require(result.returncode != 0 or result.stdout == "not-installed", "Preserve an existing application package; use a fresh distro")
    for path in [Path("/usr/bin/termterm"), Path("/usr/bin/termterm-vpn-dev"), Path("/opt/termterm-appimage"),
                 *[Path(prefix) / service for prefix in ["/etc", "/opt", "/var/lib", "/run"] for service in ["termterm-vpn", "termterm-vpn-dev"]]]:
        require(not plain(path).exists(), "Preexisting installation/session state refused")
    data = [HOME / prefix / IDENTITY for prefix in [".config", ".local/share", ".cache"]]
    for prefix in [".config", ".local/share", ".cache"]:
        for identity in ["local.termterm.desktop", "local.termterm.desktop.vpndev"]:
            require(not plain(HOME / prefix / identity).exists(), "Preexisting application user data refused")
    require(re.fullmatch(r"[a-z0-9-]{1,40}", args.run), "Simple acceptance run name required")
    plain(ROOT)
    if not ROOT.exists():
        ROOT.mkdir(mode=0o700)
    require(ROOT.stat().st_uid == 0 and not ROOT.stat().st_mode & 0o077, "Private root-owned acceptance output required")
    case = ROOT / args.run
    fresh(case, ROOT)
    old = receipt(args.previous, args.expected_previous_receipt_sha256) if args.previous else None
    new = receipt(args.current, args.expected_receipt_sha256)
    upgrade(old["receipt"]["version"] if old else None, new["receipt"]["version"])
    if old:
        require(old["receipt"]["debClientSha256"] != new["receipt"]["debClientSha256"], "Upgrade pin rejection requires two distinct installed desktop hashes")
    case.mkdir(mode=0o700)
    # Freeze reviewed installers inside this disposable guest before privileged installation.
    packages = ([("previous", old, args.expected_previous_receipt_sha256)] if old else []) + [("current", new, args.expected_receipt_sha256)]
    for label, package, pin in packages:
        target = case / label
        target.mkdir()
        for name in ["PACKAGE-RECEIPT.json", *package["names"]]:
            shutil.copyfile(package["directory"] / name, target / name)
        frozen = receipt(target, pin)
        package.update(frozen)
        inspect(package)
    report = {"ok": False, "identity": IDENTITY, "previousVersion": old["receipt"]["version"] if old else None, "version": new["receipt"]["version"],
              "candidateVersion": CANDIDATE, "upgradeTested": bool(old), "distro": distro, "vaultOpened": False,
              "nativeVpnTested": False, "liveVpnUiCloseTested": False, "appImageBrokerPairingTested": False, "appImageWindowAndClose": False}
    attempted = False
    try:
        for path in data:
            path.mkdir(parents=True, mode=0o700)
            for ancestor in [path, *list(path.parents)[:2]]:
                if ancestor.is_relative_to(HOME) and ancestor != HOME:
                    os.chown(ancestor, 1000, 1000)
            sentinel = path / "installer-acceptance-sentinel"
            sentinel.write_text("preserve-disposable-development-data\n")
            os.chown(sentinel, 1000, 1000)
        for index, (_, package, _) in enumerate(packages):
            attempted = True
            command(["dpkg", "--force-confold", "--install", *[package["directory"] / name for name in package["names"][:2]]], timeout=180)
            installed(package)
            if index:
                require(CONFIG.read_bytes() == retained, "Upgrade replaced administrator-modified broker configuration")
                stale_pin_rejected(command(["python3", "-I", BROKER, "--check"], check=False))
                report["upgradePreservedConfigAndRejectedStalePins"] = True
                # Explicit admin repin in the disposable guest; retain the prior config as evidence.
                empty_journal()
                CONFIG.write_bytes(package["config"])
                CONFIG.chmod(0o600)
            else:
                require(CONFIG.read_bytes() == package["config"], "Installed broker config differs")
            check_service()
            window(CLIENT, case, "deb-" + str(index))
            for path in data:
                require((path / "installer-acceptance-sentinel").read_text() == "preserve-disposable-development-data\n", "Upgrade changed disposable retained data")
            if not index:
                retained = CONFIG.read_bytes() + b"\n"
                CONFIG.write_bytes(retained)
                (case / "retained-administrator-config.json").write_bytes(retained)
        report.update(installedPayloadVerified=True, packagedDefaultBrokerStartedAndStopped=True, realWindowsAndGracefulClose=True, upgradeRetainedData=bool(old))
        if not CANDIDATE:
            appimage = new["directory"] / new["names"][2]
            appimage.chmod(0o755)
            # The development AppImage is a separate preview, not the broker's pinned /usr/bin client.
            window(appimage, case, "appimage")
            report["appImageWindowAndClose"] = True
        stop_service()
        command(["dpkg", "--remove", PACKAGES[1], PACKAGES[0]], timeout=180)
        attempted = False
        require(not CLIENT.exists() and not BROKER.exists() and CONFIG.is_file(), "Package removal failed or removed retained config")
        if CANDIDATE:
            require(CONFIG.read_bytes() == retained, "Removal changed administrator-modified candidate configuration")
        for path in data:
            require((path / "installer-acceptance-sentinel").is_file(), "Removal discarded retained disposable data")
        report.update(ok=True, uninstalled=True, configurationAndUserDataRetained=True)
    except Exception as error:
        report["failure"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        if attempted:
            try:
                stop_service()
                command(["dpkg", "--remove", PACKAGES[1], PACKAGES[0]], timeout=180)
                report["failureCleanupRemovedOwnedPackages"] = True
            except Exception as error:
                report["cleanupFailure"] = type(error).__name__ + ": " + str(error)
        (case / "result.json").write_text(json.dumps(report, indent=2) + "\n")
    print("PASS: disposable WSL " + ("candidate clean install/service/window/remove; " if CANDIDATE else "development install/service/window/upgrade/remove; ") + new["receipt"]["version"])


def candidate_self_test():
    configure("0.4.4")
    checks = 0

    def rejects(action):
        nonlocal checks
        try:
            action()
        except (ValueError, KeyError):
            checks += 1
        else:
            raise AssertionError("Unsafe candidate input accepted")

    try:
        require((IDENTITY, CLIENT.as_posix(), UNIT, SOCKET.as_posix()) ==
                ("local.termterm.desktop", "/usr/bin/termterm", "termterm-vpn.service", "/run/termterm-vpn/broker.sock"), "Candidate layout differs")
        checks += 1
        for version in ["", "0.4.2", "0.4.5", "0.4.3-dev.4"]:
            rejects(lambda: configure(version))
        upgrade(None, "0.4.4")
        checks += 1
        for previous, current in [("0.4.3-dev.3", "0.4.4"), ("0.4.2", "0.4.4"), ("0.4.4", "0.4.4"), (None, "0.4.5")]:
            rejects(lambda: upgrade(previous, current))
        rejects(lambda: run(argparse.Namespace(previous=Path("dev-baseline"), expected_previous_receipt_sha256=None)))
        rejects(lambda: run(argparse.Namespace(previous=None, expected_previous_receipt_sha256="a" * 64)))
        pins = {name: "b" * 64 for name in ["vpn-policy-lab", "termterm-openvpn", "termterm-openconnect", "termterm-wireguard"]}
        data = {"identity": IDENTITY, "developmentOnly": False, "brokerUid": 1000, "version": CANDIDATE,
                "candidateVersion": CANDIDATE, "desktopPackage": PACKAGES[0], "brokerService": UNIT,
                "distributionReady": False, "releaseCandidate": True, "updateChannel": "stable",
                "debClientSha256": "a" * 64, "brokerPins": pins, "files": {}}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            names = ["TermTerm_0.4.4_amd64.deb", "termterm-vpn-broker_0.4.4_amd64.deb"]
            for name in names:
                (root / name).write_bytes(b"candidate-deb")
                data["files"][name] = {"sha256": digest(root / name), "bytes": (root / name).stat().st_size}
            path = root / "PACKAGE-RECEIPT.json"
            path.write_text(json.dumps(data))
            require(receipt(root, digest(path))["names"] == names, "Candidate DEB check must not require an AppImage")
            checks += 1
            for key, value in [("identity", "local.termterm.desktop.vpndev"), ("developmentOnly", True),
                               ("version", "0.4.3-dev.4"), ("candidateVersion", "0.4.2"), ("brokerUid", 0), ("brokerUid", "1000"),
                               ("desktopPackage", "term-term-vpn-dev"), ("brokerService", "termterm-vpn-dev.service"),
                               ("distributionReady", True), ("releaseCandidate", False), ("updateChannel", "candidate"), ("brokerPins", {})]:
                path.write_text(json.dumps({**data, key: value}))
                rejects(lambda: receipt(root, digest(path)))
            path.write_text(json.dumps(data))
            rejects(lambda: receipt(root, "0" * 64))
            (root / names[0]).write_bytes(b"tampered")
            rejects(lambda: receipt(root, digest(path)))
        config = {"uid": 1000, "client": "/usr/bin/termterm", "clientSha256": "a" * 64, "engines": "/opt/termterm-vpn/bin",
                  "pins": pins, "journal": "/var/lib/termterm-vpn", "applicationIdentifier": IDENTITY, "candidateVersion": CANDIDATE}
        validate_broker_config(config, data)
        checks += 1
        for key, value in [("uid", 0), ("uid", "1000"), ("client", "/opt/termterm-appimage/usr/bin/termterm"),
                           ("engines", "/opt/termterm-vpn-dev/bin"), ("journal", "/var/lib/termterm-vpn-dev"),
                           ("clientSha256", "c" * 64), ("applicationIdentifier", "local.termterm.desktop.vpndev"),
                           ("candidateVersion", "0.4.2"), ("clientInstall", "appimage")]:
            rejects(lambda: validate_broker_config({**config, key: value}, data))
        unit = b"ExecStart=/usr/bin/python3 -I /opt/termterm-vpn/bin/vpn-unix-broker.py --candidate 0.4.4\nRuntimeDirectory=termterm-vpn\nStateDirectory=termterm-vpn\n"
        validate_candidate_unit(unit)
        checks += 1
        for raw in [unit.replace(b" --candidate 0.4.4", b""), unit + b"ExecStartPre=/bin/true\n", unit.replace(b"StateDirectory=termterm-vpn", b"StateDirectory=termterm-vpn-dev")]:
            rejects(lambda: validate_candidate_unit(raw))
        for name, broker, accepted in [("usr/bin/termterm", False, True), ("etc/termterm-vpn/broker.json", True, True),
                                       ("usr/lib/systemd/system/termterm-vpn.service", True, True),
                                       ("usr/bin/termterm-vpn-dev", False, False), ("etc/termterm-vpn-dev/broker.json", True, False),
                                       ("usr/lib/systemd/system/termterm-vpn-dev.service", True, False)]:
            raw = io.BytesIO()
            with tarfile.open(fileobj=raw, mode="w") as archive:
                member = tarfile.TarInfo(name)
                member.size = 1
                archive.addfile(member, io.BytesIO(b"x"))
            if accepted:
                require(name in members(raw.getvalue(), broker=broker), "Candidate member rejected")
                checks += 1
            else:
                rejects(lambda: members(raw.getvalue(), broker=broker))
        print(json.dumps({"candidateSelfTest": "0.4.4", "passed": checks, "packageInstalled": False, "serviceStarted": False, "wslAction": False}))
    finally:
        configure()


def self_test():
    checks = 0
    for name, marker, expected in [(name, name + "\n", True) for name in DISTROS] + [
        (DISTROS[0], DISTROS[1] + "\n", False), ("Ubuntu-24.04", "Ubuntu-24.04\n", False),
        (DISTROS[1], DISTROS[1], False), (None, "", False)
    ]:
        require(distro_matches(name, marker) is expected, "Acceptance distro/marker binding differs")
        checks += 1
    upgrade("0.4.3-dev.1", "0.4.3-dev.2")
    checks += 1
    for pair in [("0.4.3-dev.1", "0.4.3-dev.1"), ("0.4.3-dev.2", "0.4.3-dev.1"), ("0.4.4", "0.4.3-dev.2"), ("0.4.3-dev.1", "0.4.4-dev.2")]:
        try:
            upgrade(*pair)
        except ValueError:
            checks += 1
        else:
            raise AssertionError("Unsafe upgrade accepted")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary).resolve()
        existing = root / "existing"
        existing.mkdir()
        (existing / "preserved").write_bytes(b"original")
        fresh(root / "fresh", root)
        for path in [existing, root / "../outside"]:
            try:
                fresh(path, root)
            except ValueError:
                checks += 1
            else:
                raise AssertionError("Existing/escaping output accepted")
        data = {"identity": IDENTITY, "developmentOnly": True, "brokerUid": 1000, "version": "0.4.3-dev.1", "debClientSha256": "a" * 64, "files": {}}
        for name in ["TermTerm VPN Dev_0.4.3-dev.1_amd64.deb", "termterm-vpn-dev-broker_0.4.3-dev.1_amd64.deb", "TermTerm VPN Dev_0.4.3-dev.1_amd64.AppImage"]:
            (root / name).write_bytes(b"inert package fixture")
            data["files"][name] = {"sha256": digest(root / name), "bytes": (root / name).stat().st_size}
        path = root / "PACKAGE-RECEIPT.json"
        path.write_text(json.dumps(data))
        pin = digest(path)
        receipt(root, pin)
        (root / next(iter(data["files"]))).write_bytes(b"tampered")
        try:
            receipt(root, pin)
        except ValueError:
            checks += 1
        else:
            raise AssertionError("Tampered package accepted")
        data["identity"] = "local.termterm.desktop"
        path.write_text(json.dumps(data))
        try:
            receipt(root, digest(path))
        except ValueError:
            checks += 1
        else:
            raise AssertionError("Stable identity accepted")
        require((existing / "preserved").read_bytes() == b"original", "Preexisting fixture changed")
    for name in ["../escape", "/usr/bin/termterm", "usr/bin/termterm", "postinst"]:
        raw = io.BytesIO()
        with tarfile.open(fileobj=raw, mode="w") as archive:
            member = tarfile.TarInfo(name)
            member.size = 1
            archive.addfile(member, io.BytesIO(b"x"))
        try:
            members(raw.getvalue(), control=name == "postinst")
        except ValueError:
            checks += 1
        else:
            raise AssertionError("Unsafe DEB member accepted")
    failed = sp.run([sys.executable, "-c", "raise SystemExit(17)"], capture_output=True, timeout=5)
    require(failed.returncode == 17, "Native subprocess failure lost")
    checks += 1
    stale_pin_rejected(sp.CompletedProcess([], 1, stderr="ValueError: executable_pin_mismatch\n"))
    checks += 1
    for result in [sp.CompletedProcess([], 0, stderr="ValueError: executable_pin_mismatch"), sp.CompletedProcess([], 1, stderr="ValueError: trusted_path_must_be_root_owned")]:
        try:
            stale_pin_rejected(result)
        except ValueError:
            checks += 1
        else:
            raise AssertionError("Unrelated broker result counted as stale-pin evidence")
    if sys.platform == "linux":
        parent = sp.Popen([sys.executable, "-c", "import subprocess,sys; p=subprocess.Popen([sys.executable,'-c','import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); print(\"ready\",flush=True); time.sleep(60)'],stdout=subprocess.PIPE,text=True); assert p.stdout.readline().strip()=='ready'; print(p.pid,flush=True)"], stdout=sp.PIPE, text=True, start_new_session=True)
        try:
            child = int(parent.stdout.readline())
            require(parent.wait(timeout=5) == 0 and os.getpgid(child) == parent.pid, "Lingering descendant fixture failed")
            terminate_owned(parent)
            require(not group_alive(parent.pid), "Exited launcher left a live descendant")
            checks += 1
        finally:
            terminate_owned(parent)
            parent.stdout.close()
    print(json.dumps({"selfTest": True, "passed": checks, "packageInstalled": False, "serviceStarted": False, "wslAction": False}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", choices=["0.4.4"], help="Clean-install the exact production candidate in the fresh isolated guest; no upgrade claim")
    parser.add_argument("--previous", type=Path)
    parser.add_argument("--expected-previous-receipt-sha256")
    parser.add_argument("--current", type=Path)
    parser.add_argument("--expected-receipt-sha256")
    parser.add_argument("--run", default="install-upgrade-v1")
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    configure(arguments.candidate)
    if arguments.self_test:
        candidate_self_test() if CANDIDATE else self_test()
    elif arguments.current and arguments.expected_receipt_sha256 and (CANDIDATE or arguments.previous and arguments.expected_previous_receipt_sha256):
        run(arguments)
    else:
        parser.error("Provide reviewed package directories and receipt pins (current only for --candidate 0.4.4), or --self-test")
