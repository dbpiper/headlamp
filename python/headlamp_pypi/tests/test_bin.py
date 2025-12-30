from __future__ import annotations

import os
from pathlib import Path

import pytest

from headlamp._bin import get_binary_path


def test_binary_path_resolution_uses_override(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    fake = tmp_path / "fake"
    fake.write_text("x", encoding="utf-8")
    monkeypatch.setenv("HEADLAMP_PYPI_BIN_OVERRIDE", str(fake))
    assert get_binary_path() == fake

