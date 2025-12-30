from __future__ import annotations

import os
import platform
from pathlib import Path

try:
    # Python 3.9+
    from importlib import resources as importlib_resources  # type: ignore
except Exception:  # pragma: no cover
    import importlib_resources  # type: ignore


_OVERRIDE_ENV = "HEADLAMP_PYPI_BIN_OVERRIDE"


def _default_binary_resource_name() -> str:
    return "headlamp.exe" if platform.system().lower().startswith("win") else "headlamp"


def get_binary_path() -> Path:
    """
    Returns the absolute path to the bundled `headlamp` binary.

    Test override:
    - If `HEADLAMP_PYPI_BIN_OVERRIDE` is set, that path is used (if it exists).
    """

    override = os.environ.get(_OVERRIDE_ENV, "").strip()
    if override:
        p = Path(override).expanduser()
        if not p.is_absolute():
            p = (Path.cwd() / p).resolve()
        if not p.exists():
            raise FileNotFoundError(
                f"headlamp (PyPI): override binary not found at {_OVERRIDE_ENV}={p}"
            )
        return p

    resource_name = _default_binary_resource_name()

    try:
        # pkg is headlamp.bin (a namespace for resources)
        bin_dir = importlib_resources.files("headlamp").joinpath("bin")
        candidate = bin_dir.joinpath(resource_name)
        candidate_path = Path(str(candidate))
    except Exception as e:  # pragma: no cover
        raise FileNotFoundError(
            "headlamp (PyPI): could not resolve bundled binary resources"
        ) from e

    if not candidate_path.exists():
        raise FileNotFoundError(_missing_binary_message(candidate_path))
    return candidate_path


def _missing_binary_message(expected_path: Path) -> str:
    return (
        "headlamp (PyPI): bundled headlamp binary not found.\n"
        f"Expected at: {expected_path}\n\n"
        "This usually means you installed an sdist (source distribution) or a wheel for a\n"
        "different platform.\n\n"
        "Fix:\n"
        "  - Reinstall wheels only: pip install --only-binary :all: headlamp\n"
    )

