#!/usr/bin/env python3
"""Build a separate, auth-only diagnostic CLI from the cached pinned toolchain."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / ".tools/vpn-windows"


def replace_once(text, old, new):
    if text.count(old) != 1:
        raise ValueError("unexpected_pinned_source")
    return text.replace(old, new, 1)


def main():
    lock = json.loads((ROOT / "scripts/vpn-windows-dependencies.json").read_bytes())
    archive = TOOLS / "openconnect-9.21.tar.gz"
    if hashlib.sha256(archive.read_bytes()).hexdigest() != lock["openconnect"]["sourceSha256"]:
        raise ValueError("source_archive_hash_mismatch")
    with tarfile.open(archive) as source:
        text = source.extractfile("openconnect-9.21/main.c").read().decode()
    text = replace_once(text, "#include <time.h>\n",
                        '#include <time.h>\n#include "termterm-auth-guard.h"\n')
    text = replace_once(text, "\tif (!vpninfo->cookie && openconnect_obtain_cookie(vpninfo)) {",
        "\tif (cookieonly != 3 || !non_inter || !vpninfo->no_http_keepalive ||\n"
        "\t    strcmp(vpninfo->proto->name, \"fortinet\") || !password || username)\n"
        "\t\texit(2);\n"
        "\ttermterm_auth_host = strdup(vpninfo->hostname);\n"
        "\ttermterm_auth_port = vpninfo->port;\n"
        "\tif (!vpninfo->cookie && openconnect_obtain_cookie(vpninfo)) {")
    text = replace_once(text, "\tint empty = 1;\n\n\tif (!form->auth_id)",
        "\tint empty = 1;\n"
        "\tenum termterm_auth_result guard = termterm_auth_check(vpninfo->hostname, vpninfo->port, form);\n"
        "\tif (guard != TERMTERM_AUTH_INITIAL) {\n"
        "\t\ttermterm_auth_exit = guard == TERMTERM_AUTH_OTP ? 42 :\n"
        "\t\t    guard == TERMTERM_AUTH_ORIGIN ? 43 : guard == TERMTERM_AUTH_STOP ? 44 : 45;\n"
        "\t\treturn OC_FORM_RESULT_CANCELLED;\n"
        "\t}\n"
        "\ttermterm_auth_submitted = 1;\n\n\tif (!form->auth_id)")
    text = replace_once(text, '\t\tfprintf(stderr, _("Failed to complete authentication\\n"));\n\t\texit(1);',
        '\t\tfprintf(stderr, _("Failed to complete authentication\\n"));\n\t\texit(termterm_auth_exit ? termterm_auth_exit : 1);')
    build = TOOLS / "openconnect-build"
    for path, content in [(build / "main.c", text.encode()),
                          (build / "termterm-auth-guard.h", (ROOT / "scripts/openconnect-auth-guard.h").read_bytes())]:
        if path.exists() and path.read_bytes() != content:
            previous = TOOLS / "auth-probe-v1" / path.name
            if not previous.exists() or previous.read_bytes() != path.read_bytes():
                raise ValueError("existing_diagnostic_source_differs")
        if not path.exists() or path.read_bytes() != content:
            path.write_bytes(content)
    msys = TOOLS / "msys2-portable/msys64"
    # This only adds CLI objects in the existing generated build; no DLL target changes.
    env = dict(os.environ, MSYSTEM="MINGW64")
    posix = "/" + build.drive[0].lower() + build.as_posix()[2:]
    subprocess.run([str(msys / "usr/bin/bash.exe"), "--login", "-c",
                    "cd '" + posix + "' && make -o version.c -o libopenconnect.la -j2 openconnect.exe"],
                   env=env, check=True, timeout=120)
    output = TOOLS / "auth-probe-v2"
    output.mkdir(exist_ok=True)
    binary = build / ".libs/openconnect.exe"
    if not binary.exists():
        binary = build / "openconnect.exe"
    destination = output / "openconnect-auth-probe.exe"
    if destination.exists():
        raise ValueError("existing_diagnostic_binary_preserved")
    shutil.copy2(binary, destination)
    for dependency in (TOOLS / "engine").glob("*.dll"):
        shutil.copy2(dependency, output / dependency.name)
    print(json.dumps({"binarySha256": hashlib.sha256(destination.read_bytes()).hexdigest(),
                      "dllSha256": hashlib.sha256((output / "libopenconnect-5.dll").read_bytes()).hexdigest(),
                      "guardSha256": hashlib.sha256((ROOT / "scripts/openconnect-auth-guard.h").read_bytes()).hexdigest()}))


if __name__ == "__main__":
    main()
