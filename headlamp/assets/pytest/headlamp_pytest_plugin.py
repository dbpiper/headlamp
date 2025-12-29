import json
import sys


EVENT_PREFIX = "HEADLAMP_PYTEST_EVENT "


def _emit(payload):
    try:
        line = EVENT_PREFIX + json.dumps(payload, ensure_ascii=False)
    except Exception as exc:
        line = EVENT_PREFIX + json.dumps({"type": "error", "message": str(exc)})
    sys.__stdout__.write(line + "\n")
    sys.__stdout__.flush()


def pytest_runtest_logreport(report):
    if getattr(report, "when", None) != "call":
        return
    payload = {
        "type": "case",
        "nodeid": getattr(report, "nodeid", ""),
        "outcome": getattr(report, "outcome", ""),
        "duration": float(getattr(report, "duration", 0.0) or 0.0),
        "stdout": getattr(report, "capstdout", "") or "",
        "stderr": getattr(report, "capstderr", "") or "",
    }
    if payload["outcome"] == "failed":
        payload["longrepr"] = getattr(report, "longreprtext", "") or ""
    _emit(payload)


def pytest_runtest_logstart(nodeid, location):
    # Emit a lightweight "currently running" hint so headlamp can show per-test progress even in
    # quiet mode (-q). This is emitted before the test body runs.
    _emit(
        {
            "type": "case_start",
            "nodeid": nodeid or "",
        }
    )
