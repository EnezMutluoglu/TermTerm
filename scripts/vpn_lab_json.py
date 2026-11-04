"""Private, complete JSON snapshots shared by the disposable VPN lab processes."""
import errno
import json
import os
from pathlib import Path
import tempfile
import time


def retry_io(operation, reading=False, replacing=False):
    for attempt in range(100):
        try:
            return operation()
        except OSError as failure:
            code = getattr(failure, "winerror", None)
            transient = code in (32, 33) or (code is None and failure.errno == errno.EACCES)
            transient |= reading and failure.errno == errno.ENOENT
            # ReplaceFileW 1175 retains both names; 1176/1177 can leave partial state.
            transient |= replacing and code == 1175
            if not transient or attempt == 99:
                raise
            time.sleep(.01)


def read_json(path):
    data = retry_io(Path(path).read_bytes, reading=True)
    return json.loads(data)


def replace(temporary, path):
    if os.name == "nt" and path.exists():
        import ctypes
        from ctypes import wintypes
        function = ctypes.WinDLL("kernel32", use_last_error=True).ReplaceFileW
        function.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.DWORD, wintypes.LPVOID, wintypes.LPVOID]
        function.restype = wintypes.BOOL
        if not function(str(path), str(temporary), None, 0, None, None):
            raise ctypes.WinError(ctypes.get_last_error())
    else:
        temporary.replace(path)


def write_json(path, value, exclusive=False):
    path = Path(path)
    descriptor, name = tempfile.mkstemp(prefix="." + path.name + ".", suffix=".tmp", dir=path.parent)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as output:
            json.dump(value, output)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        if exclusive:
            retry_io(lambda: os.link(temporary, path))
        else:
            retry_io(lambda: replace(temporary, path), replacing=True)
        if os.name == "posix":
            directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
    finally:
        temporary.unlink(missing_ok=True)
