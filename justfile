


example name = "basic":
    cargo run -p examples --bin {{name}}

setup-cross:
    cargo +nightly -Zscript scripts/setup-cross.rs

check-macos:       (_zig-check "aarch64-apple-darwin")
check-linux:       (_zig-check "x86_64-unknown-linux-gnu")
check-linux-arm:   (_zig-check "aarch64-unknown-linux-gnu")

build-macos:       (_zig-build "aarch64-apple-darwin")
build-linux:       (_zig-build "x86_64-unknown-linux-gnu")
build-linux-arm:   (_zig-build "aarch64-unknown-linux-gnu")

clippy-macos:      (_zig-clippy "aarch64-apple-darwin")
clippy-linux:      (_zig-clippy "x86_64-unknown-linux-gnu")
clippy-linux-arm:  (_zig-clippy "aarch64-unknown-linux-gnu")

_zig-check target:
    cargo zigbuild --target {{target}} --workspace

_zig-build target:
    cargo zigbuild --target {{target}} --workspace --release

_zig-clippy target:
    cargo clippy --target {{target}} --workspace --all-targets