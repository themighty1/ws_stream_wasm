#!/usr/bin/bash

# Regression test for "ws messages arriving before WsStream is constructed".
#
# Runs tests/atomics_repro.rs under the multithreaded wasm-bindgen-futures
# driver (target_feature = "atomics"). The driver swap is purely a compile-time
# decision, but a shared-memory module needs explicit linker flags (the same
# set threaded-wasm builds use): --shared-memory so the memory is actually
# shared, --import-memory so wasm-bindgen's threading transform can inject the
# thread id, plus the TLS/__heap_base exports it requires.
#
# Needs: nightly Rust, wasm32-unknown-unknown target, node (for ci/greeter.js),
# and Chrome + matching chromedriver (CHROMEDRIVER must point at it, as in the
# standard wasm-pack setup).

# fail fast
#
set -e

# print each command before it's executed
#
set -x

export RUSTUP_TOOLCHAIN=nightly

# -Zbuild-std needs rust-src; the workflow also requests it, but installing
# here keeps local runs working and makes CI robust to toolchain-action quirks.
#
rustup component add rust-src --toolchain nightly
rustup target add wasm32-unknown-unknown --toolchain nightly

export RUSTFLAGS='-Ctarget-feature=+atomics,+bulk-memory,+mutable-globals -Clink-arg=--shared-memory -Clink-arg=--max-memory=4294967296 -Clink-arg=--import-memory -Clink-arg=--export=__heap_base -Clink-arg=--export=__wasm_init_tls -Clink-arg=--export=__tls_size -Clink-arg=--export=__tls_align -Clink-arg=--export=__tls_base --cfg getrandom_backend="wasm_js"'

# chromedriver/chrome fail to bind the DevTools socket on hosts without IPv6
# loopback (EADDRNOTAVAIL); pinning to 127.0.0.1 fixes that and is harmless
# elsewhere.
#
export CHROMEDRIVER_ARGS="--allowed-ips=127.0.0.1 --remote-debugging-address=127.0.0.1"

node ci/greeter.js &
GREETER_PID=$!
trap "kill $GREETER_PID 2>/dev/null || true" EXIT

# Wait for the greeter to accept connections before starting the browser test.
#
for i in $(seq 1 50); do
  (echo > /dev/tcp/127.0.0.1/3313) >/dev/null 2>&1 && break
  sleep 0.2
done

wasm-pack test --chrome --headless -- \
  --target wasm32-unknown-unknown \
  --no-default-features \
  --test atomics_repro \
  -Zbuild-std=std,panic_abort