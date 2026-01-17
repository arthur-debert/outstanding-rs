#!/bin/bash
set -e

echo "=== CI Checks ==="

echo "[1/6] cargo check (all packages)"
cargo check --all --quiet

echo "[2/6] cargo test (default features)"
cargo test --all --quiet

echo "[3/6] cargo test (macros feature)"
cargo test -p standout --features macros --quiet

echo "[4/6] cargo test (clap feature)"
cargo test -p standout --features clap --quiet

echo "[5/6] cargo fmt --check"
cargo fmt --all -- --check

echo "[6/6] cargo clippy (all features)"
cargo clippy --all --all-features --quiet -- -D warnings

echo "=== All checks passed ==="
