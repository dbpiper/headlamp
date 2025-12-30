from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest


def _run_module_headlamp_cli(
    args: list[str], env: dict[str, str]
) -> subprocess.CompletedProcess[str]:
    cmd = [sys.executable, "-c", "import headlamp._cli as c; c.main()"]
    return subprocess.run(cmd + args, env=env, text=True, capture_output=True)


def test_missing_binary_message_when_override_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    missing = tmp_path / "does-not-exist"
    env = os.environ.copy()
    env["HEADLAMP_PYPI_BIN_OVERRIDE"] = str(missing)
    p = _run_module_headlamp_cli(["--help"], env)
    assert p.returncode != 0
    assert "override binary not found" in (p.stderr + p.stdout)


def test_argument_passthrough_via_override(tmp_path: Path) -> None:
    # Create a tiny fake "binary" that echoes argv to a file.
    out = tmp_path / "out.txt"
    if sys.platform.startswith("win"):
        fake = tmp_path / "fake.cmd"
        fake.write_text(
            f'@echo off\r\necho %* > "{out}"\r\nexit /b 7\r\n', encoding="utf-8"
        )
    else:
        fake = tmp_path / "fake.sh"
        fake.write_text(f'#!/bin/sh\necho "$@" > "{out}"\nexit 7\n', encoding="utf-8")
        fake.chmod(0o755)

    env = os.environ.copy()
    env["HEADLAMP_PYPI_BIN_OVERRIDE"] = str(fake)
    p = _run_module_headlamp_cli(["hello", "world"], env)
    assert p.returncode == 7
    assert out.read_text(encoding="utf-8").strip() == "hello world"
