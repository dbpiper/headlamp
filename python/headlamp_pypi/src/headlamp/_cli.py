from __future__ import annotations

import os
import subprocess
import sys

from ._bin import get_binary_path


def main() -> None:
    try:
        bin_path = get_binary_path()
    except FileNotFoundError as e:
        sys.stderr.write(str(e).rstrip() + os.linesep)
        raise SystemExit(1)

    argv = [str(bin_path), *sys.argv[1:]]
    try:
        completed = subprocess.run(argv)
    except FileNotFoundError:
        sys.stderr.write(f"headlamp (PyPI): failed to execute: {bin_path}{os.linesep}")
        raise SystemExit(1)

    raise SystemExit(completed.returncode)

