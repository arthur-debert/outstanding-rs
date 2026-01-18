#!/bin/bash
set -e

echo "=== CI Checks ==="

echo "[1/5] cargo check (all packages)"
cargo check --all --quiet

echo "[2/5] cargo fmt --check"
cargo fmt --all -- --check

echo "[3/5] cargo clippy (all features)"
cargo clippy --all --all-features --quiet -- -D warnings

echo "[4/5] cargo test (lib tests)"
# Run lib tests for each crate explicitly (avoid --all which can include doctests)
cargo test -p standout --lib --quiet
cargo test -p standout-render --lib --quiet
cargo test -p standout-bbparser --lib --quiet
cargo test -p standout-macros --lib --quiet

echo "[5/5] cargo test (doctests and integration tests)"
# Run doctests for standout (not standout-render, as its docs reference standout::)
cargo test -p standout --doc --quiet
cargo test -p standout-bbparser --doc --quiet
# Run integration tests
cargo test -p standout --test '*' --quiet

echo "=== All checks passed ==="
