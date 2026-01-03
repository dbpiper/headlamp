FROM rust:1.92-bookworm

ENV DEBIAN_FRONTEND=noninteractive
ENV PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    gnupg \
    mold \
    python3 \
    python3-venv \
    sccache \
    && rm -rf /var/lib/apt/lists/*

# Ensure login shells keep Rust on PATH (some /etc/profile setups reset PATH).
RUN printf '%s\n' \
    'export PATH=/usr/local/cargo/bin:/usr/local/rustup/bin:$PATH' \
    > /etc/profile.d/zz-headlamp-rust-path.sh

# Node 20 (for Jest parity runner)
RUN mkdir -p /etc/apt/keyrings \
    && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
    | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
    && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_20.x nodistro main" \
    > /etc/apt/sources.list.d/nodesource.list \
    && apt-get update -y \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Preinstall Jest deps into an image-global location so CI doesn't need `npm ci`.
WORKDIR /opt/headlamp/js_deps
COPY headlamp_tests/tests/js_deps/package.json ./package.json
COPY headlamp_tests/tests/js_deps/package-lock.json ./package-lock.json
RUN npm ci --silent

# Preinstall pytest deps into an image-global venv so CI doesn't need network/pip.
WORKDIR /opt/headlamp/py_deps
COPY headlamp_tests/tests/py_deps/requirements.txt ./requirements.txt
RUN python3 -m venv /opt/headlamp/py_venv \
    && /opt/headlamp/py_venv/bin/pip install --no-input --disable-pip-version-check -r requirements.txt

# Rust components + cargo tools used by CI and parity runs
RUN rustup component add rustfmt clippy llvm-tools-preview

# Install cargo tools via prebuilt binaries for fast image builds.
# (Compiling these from source dominates build time.)
RUN curl -fsSL https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
    | bash \
    && cargo binstall -y --no-symlinks --locked cargo-nextest cargo-llvm-cov

