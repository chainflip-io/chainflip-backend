apt-get -y install --no-install-recommends \
        cmake \
        build-essential \
        clang \
        libclang-dev \
        lld \
        python3-dev

SCCACHE_VER=0.2.15
curl -fsSLo /tmp/sccache.tgz \
 https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VER}/sccache-v${SCCACHE_VER}-x86_64-unknown-linux-musl.tar.gz \
 && tar -xzf /tmp/sccache.tgz -C /tmp --strip-components=1 \
 && mv /tmp/sccache /usr/bin && chmod +x /usr/bin/sccache \
 && rm -rf /tmp/*

export RUSTC_WRAPPER=sccache
NIGHTLY=nightly-2021-03-24
rustup default ${NIGHTLY} \
    && rustup target add wasm32-unknown-unknown --toolchain ${NIGHTLY} \
    && rustup component add rustfmt

cargo build --release