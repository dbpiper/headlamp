#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

IMAGE="${IMAGE:-headlamp-ci:local}"
PLATFORM="${PLATFORM:-linux/amd64}"

VOLUME_CARGO_HOME="${VOLUME_CARGO_HOME:-headlamp-cargo-home}"
VOLUME_TARGET_DIR="${VOLUME_TARGET_DIR:-headlamp-cargo-target}"
VOLUME_SCCACHE="${VOLUME_SCCACHE:-headlamp-sccache}"

docker volume create "${VOLUME_CARGO_HOME}" >/dev/null
docker volume create "${VOLUME_TARGET_DIR}" >/dev/null
docker volume create "${VOLUME_SCCACHE}" >/dev/null

DOCKER_TTY_FLAGS=()
if [[ -t 1 ]]; then
	DOCKER_TTY_FLAGS=(-it)
fi

echo "headlamp: docker parity run"
echo "  repo:      ${REPO_ROOT}"
echo "  image:     ${IMAGE}"
echo "  platform:  ${PLATFORM}"
echo "  cargo_home:docker volume ${VOLUME_CARGO_HOME}"
echo "  target:    docker volume ${VOLUME_TARGET_DIR}"
echo "  sccache:   docker volume ${VOLUME_SCCACHE}"
echo

docker run --rm ${DOCKER_TTY_FLAGS:+${DOCKER_TTY_FLAGS[@]}} --platform "${PLATFORM}" \
	-v "${REPO_ROOT}":/work -w /work \
	-v "${VOLUME_CARGO_HOME}":/cargo-home \
	-v "${VOLUME_TARGET_DIR}":/cargo-target \
	-v "${VOLUME_SCCACHE}":/sccache \
	-e CARGO_HOME=/cargo-home \
	-e CARGO_TARGET_DIR=/cargo-target \
	-e HEADLAMP_PARITY_DUMP_ROOT=/work/ci-dumps \
	-e RUSTC_WRAPPER=sccache \
	-e SCCACHE_DIR=/sccache \
	-e SCCACHE_CACHE_SIZE=20G \
	-e CARGO_INCREMENTAL=1 \
	"${IMAGE}" \
	bash -lc '
    set -euo pipefail
    git config --global --add safe.directory /work || true
    rm -rf /work/headlamp_parity_support/target/parity-fixtures/worktrees/git-lock || true
    echo "headlamp: inside container"
    echo "  rust:    $(rustc -V)"
    echo "  cargo:   $(cargo -V)"
    echo "  nextest: $(cargo nextest --version | head -n 1)"
    echo "  nightly: $(rustup run nightly rustc -V 2>/dev/null || echo missing)"
    echo "  sccache: $(sccache --version)"
    sccache --zero-stats >/dev/null 2>&1 || true
    echo

    cargo build -q -p headlamp
    export HEADLAMP_PARITY_HEADLAMP_BIN=/cargo-target/debug/headlamp
    test -x "${HEADLAMP_PARITY_HEADLAMP_BIN}"

    /cargo-target/debug/headlamp --runner=headlamp headlamp_parity_tests/tests/parity_suite_test.rs
    echo
    sccache --show-stats || true
  '
