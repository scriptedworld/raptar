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
alias c := checks
alias f := fmt

# ============================================================
# Setup
# ============================================================

# Install all dev dependencies (cargo tools used by this justfile)
tools-setup:
    #!/usr/bin/env bash
    set -e
    echo "Installing dev dependencies..."
    cargo install cargo-audit
    cargo install cargo-deny
    cargo install cargo-outdated
    cargo install cargo-watch
    cargo install cargo-machete
    echo ""
    echo "Installing rust-code-analysis-cli..."
    # --locked is a workaround for tree-sitter build failures with newer deps
    # See: https://github.com/mozilla/rust-code-analysis/issues/1054
    cargo install --locked rust-code-analysis-cli || echo "⚠ rust-code-analysis-cli failed to install"
    echo ""
    echo "Installing cargo-udeps (requires nightly)..."
    cargo +nightly install --force cargo-udeps || echo "⚠ cargo-udeps failed (nightly Rust required: rustup install nightly)"
    echo ""
    echo "Done! Run 'just tools-check' to verify installation."

# Update all dev dependencies (force reinstall)
tools-update:
    #!/usr/bin/env bash
    set -e
    echo "Updating dev dependencies..."
    cargo install --force cargo-audit
    cargo install --force cargo-deny
    cargo install --force cargo-outdated
    cargo install --force cargo-watch
    cargo install --force cargo-machete
    echo ""
    echo "Installing rust-code-analysis-cli..."
    # --locked is a workaround for tree-sitter build failures with newer deps
    # See: https://github.com/mozilla/rust-code-analysis/issues/1054
    cargo install --force --locked rust-code-analysis-cli || echo "⚠ rust-code-analysis-cli failed to install"
    echo ""
    echo "Updating cargo-udeps (requires nightly)..."
    cargo +nightly install --force cargo-udeps || echo "⚠ cargo-udeps failed (nightly Rust required: rustup install nightly)"
    echo ""
    echo "Done! Run 'just tools-check' to verify installation."

# Check which dev tools are installed
tools-check:
    #!/usr/bin/env bash
    echo "Checking dev tools..."
    echo ""
    for tool in cargo-audit cargo-deny cargo-outdated cargo-watch cargo-machete rust-code-analysis-cli cargo-udeps; do
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
release: checks
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

# Static analysis (lints, dead code, complexity, security, deps)
quality: clippy-all audit deny machete complexity
    @echo ""
    @echo "✓ Quality checks passed!"

# Full check suite (format + tests + quality)
checks: fmt-check test quality
    @echo ""
    @echo "✓ All checks passed!"

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

# Cyclomatic complexity analysis
# NOTE: --locked required due to upstream tree-sitter compat issues
# See: https://github.com/mozilla/rust-code-analysis/issues/1054
complexity:
    #!/usr/bin/env bash
    if command -v rust-code-analysis-cli &> /dev/null; then
        rust-code-analysis-cli -m -p src/
    else
        echo "⚠ Skipping cyclomatic complexity (rust-code-analysis-cli not available)"
    fi

# Check for unused dependencies (fast, no nightly needed)
machete:
    #!/usr/bin/env bash
    if ! command -v cargo-machete &> /dev/null; then
        echo "Installing cargo-machete..."
        cargo install cargo-machete
    fi
    cargo machete

# ============================================================
# Security & Dependencies
# ============================================================

# Security audit
audit:
    #!/usr/bin/env bash
    if ! command -v cargo-audit &> /dev/null; then
        echo "Installing cargo-audit..."
        cargo install cargo-audit
    fi
    cargo audit

# License and dependency check
deny:
    #!/usr/bin/env bash
    if ! command -v cargo-deny &> /dev/null; then
        echo "Installing cargo-deny..."
        cargo install cargo-deny
    fi
    cargo deny check

# Check for outdated dependencies
outdated:
    #!/usr/bin/env bash
    if ! command -v cargo-outdated &> /dev/null; then
        echo "Installing cargo-outdated..."
        cargo install cargo-outdated
    fi
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

# Check for unused dependencies (requires nightly)
udeps:
    #!/usr/bin/env bash
    if ! cargo +nightly --version &> /dev/null; then
        echo "⚠ Nightly Rust required for cargo-udeps. Install with: rustup install nightly"
        exit 1
    fi
    if ! cargo +nightly udeps --version &> /dev/null; then
        echo "Installing cargo-udeps..."
        cargo +nightly install cargo-udeps
    fi
    cargo +nightly udeps

# ============================================================
# Release helpers
# ============================================================

# Create a release tarball using raptar itself
dist: release
    ./target/release/raptar -r -o raptar-$(cargo pkgid | cut -d'#' -f2).tar.gz .

# Full CI-like check (same as checks)
ci: checks

# ============================================================
# Development helpers
# ============================================================

# Watch for changes and run tests
watch:
    #!/usr/bin/env bash
    if ! command -v cargo-watch &> /dev/null; then
        echo "Installing cargo-watch..."
        cargo install cargo-watch
    fi
    cargo watch -x test

# Watch and run clippy
watch-clippy:
    #!/usr/bin/env bash
    if ! command -v cargo-watch &> /dev/null; then
        echo "Installing cargo-watch..."
        cargo install cargo-watch
    fi
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
