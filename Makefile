# raptar Makefile
# Run quality checks and common tasks

.PHONY: all build release test check fmt clippy audit deny outdated clean help

# Default target
all: check test build

# Build debug version
build:
	cargo build

# Build release version
release:
	cargo build --release

# Run all tests
test:
	cargo test

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

# Run clippy with all targets (including tests)
clippy-all:
	cargo clippy --all-targets -- -D warnings

# Security audit (requires: cargo install cargo-audit)
audit:
	cargo audit

# License and dependency check (requires: cargo install cargo-deny)
deny:
	cargo deny check

# Check for outdated dependencies (requires: cargo install cargo-outdated)
outdated:
	cargo outdated

# Run all quality tools (requires external tools installed)
quality: check audit deny

# Generate documentation
doc:
	cargo doc --no-deps --open

# Clean build artifacts
clean:
	cargo clean

# Install the binary locally
install:
	cargo install --path .

# Run the binary (for quick testing)
run:
	cargo run -- --help

# Create a release tarball using raptar itself
dist: release
	./target/release/raptar -r -o raptar-$$(cargo pkgid | cut -d'#' -f2).tar.gz .

# Help
help:
	@echo "raptar build targets:"
	@echo ""
	@echo "  build      - Build debug version"
	@echo "  release    - Build release version"
	@echo "  test       - Run all tests"
	@echo "  check      - Run format check + clippy + tests"
	@echo "  fmt        - Format code with rustfmt"
	@echo "  fmt-check  - Check formatting without changes"
	@echo "  clippy     - Run clippy linter"
	@echo "  clippy-all - Run clippy on all targets"
	@echo "  audit      - Security vulnerability check (needs cargo-audit)"
	@echo "  deny       - License/dependency check (needs cargo-deny)"
	@echo "  outdated   - Check for outdated deps (needs cargo-outdated)"
	@echo "  quality    - Run all quality tools"
	@echo "  doc        - Generate and open documentation"
	@echo "  clean      - Remove build artifacts"
	@echo "  install    - Install binary locally"
	@echo "  dist       - Create release tarball"
	@echo "  help       - Show this help"
	@echo ""
	@echo "Install quality tools:"
	@echo "  cargo install cargo-audit cargo-deny cargo-outdated"
