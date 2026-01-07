# raptar justfile
# Install just: cargo install just
# Run: just <recipe>

# Default recipe - show available commands
default:
    @just --list

# Aliases
alias b := build
alias r := run
alias t := test
alias c := check
alias f := fmt

# ============================================================
# Setup
# ============================================================

# Install all dev dependencies (cargo tools used by this justfile)
setup:
    @echo "Installing dev dependencies..."
    cargo install cargo-audit
    cargo install cargo-deny
    cargo install cargo-outdated
    cargo install cargo-watch
    @echo ""
    @echo "Optional (requires nightly):"
    @echo "  cargo install cargo-udeps"
    @echo ""
    @echo "Done! Run 'just' to see available commands."

# Check which dev tools are installed
check-tools:
    #!/usr/bin/env bash
    echo "Checking dev tools..."
    echo ""
    for tool in cargo-audit cargo-deny cargo-outdated cargo-watch cargo-udeps; do
        if cargo install --list | grep -q "^$tool "; then
            echo "✓ $tool"
        else
            echo "✗ $tool (not installed)"
        fi
    done

# ============================================================
# Core commands
# ============================================================

# Build debug version
build:
    cargo build

# Build release version (runs checks first)
release: check
    cargo build --release

# Run with arguments
run *ARGS:
    cargo run -- {{ARGS}}

# Run all tests
test:
    cargo test

# Run tests with output shown
test-verbose:
    cargo test -- --nocapture

# ============================================================
# Quality checks
# ============================================================

# Run all quality checks (format check + clippy + tests)
check: fmt-check clippy test

# Format code
fmt:
    cargo fmt

# Check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# Run clippy linter
clippy:
    cargo clippy -- -D warnings

# Run clippy on all targets including tests
clippy-all:
    cargo clippy --all-targets -- -D warnings

# Fix clippy warnings automatically where possible
clippy-fix:
    cargo clippy --fix --allow-dirty

# ============================================================
# Security & Dependencies
# ============================================================

# Security audit (requires: cargo install cargo-audit)
audit:
    cargo audit

# License and dependency check (requires: cargo install cargo-deny)
deny:
    cargo deny check

# Check for outdated dependencies (requires: cargo install cargo-outdated)
outdated:
    cargo outdated

# Update dependencies
update:
    cargo update

# ============================================================
# Documentation
# ============================================================

# Generate and open documentation
doc:
    cargo doc --no-deps --open

# Generate documentation without opening
doc-build:
    cargo doc --no-deps

# ============================================================
# Utilities
# ============================================================

# Clean build artifacts
clean:
    cargo clean

# Install the binary locally
install:
    cargo install --path .

# Uninstall the binary
uninstall:
    cargo uninstall raptar

# Show dependency tree
tree:
    cargo tree

# Check for unused dependencies (requires: cargo install cargo-udeps)
udeps:
    cargo +nightly udeps

# ============================================================
# Release helpers
# ============================================================

# Create a release tarball using raptar itself
dist: release
    ./target/release/raptar -r -o raptar-$(cargo pkgid | cut -d'#' -f2).tar.gz .

# Run all quality tools (requires external tools)
quality: check audit deny

# Full CI-like check
ci: fmt-check clippy-all test

# ============================================================
# Development helpers
# ============================================================

# Watch for changes and run tests (requires: cargo install cargo-watch)
watch:
    cargo watch -x test

# Watch and run clippy (requires: cargo install cargo-watch)
watch-clippy:
    cargo watch -x clippy

# Run with example - preview current directory
preview:
    cargo run -- --preview --size

# Run with example - create test archive
demo:
    cargo run -- -o /tmp/demo.tar.gz --verbose .
    @echo ""
    @echo "Created /tmp/demo.tar.gz"
    @ls -lh /tmp/demo.tar.gz
    @tar -tzf /tmp/demo.tar.gz

# Compare compression formats
compare-formats:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release
    echo "Comparing compression formats on current directory..."
    echo ""
    for fmt in tar tar.gz tar.bz2 tar.zst zip; do
        ./target/release/raptar -f "$fmt" -o "/tmp/test.$fmt" -q .
        size=$(ls -lh "/tmp/test.$fmt" | awk '{print $5}')
        printf "%-10s %s\n" "$fmt:" "$size"
        rm "/tmp/test.$fmt"
    done

# ============================================================
# Ecosystem Templates
# ============================================================

# Download ecosystem gitignore templates from GitHub
fetch-ecosystems:
    @echo "Downloading github/gitignore repository..."
    @mkdir -p ecosystems
    curl -sL "https://github.com/github/gitignore/archive/refs/heads/main.zip" -o /tmp/gitignore.zip
    unzip -q -o /tmp/gitignore.zip -d /tmp
    cp /tmp/gitignore-main/*.gitignore ecosystems/
    cp /tmp/gitignore-main/Global/*.gitignore ecosystems/
    @echo "# Ecosystem gitignore templates" > ecosystems/MANIFEST
    @echo "# Source: https://github.com/github/gitignore" >> ecosystems/MANIFEST
    @echo "# Downloaded: $(date -u +%Y-%m-%d)" >> ecosystems/MANIFEST
    @ls ecosystems/*.gitignore | wc -l | xargs echo "Templates downloaded:"
    @rm -rf /tmp/gitignore.zip /tmp/gitignore-main

# Show currently downloaded ecosystems
list-ecosystems:
    @cat ecosystems/MANIFEST 2>/dev/null || echo "No ecosystems downloaded. Run: just fetch-ecosystems"
