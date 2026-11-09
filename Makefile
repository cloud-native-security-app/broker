.PHONY: verify env fmt lint test test-integration doc

# Verification sequence for the broker crate. Integration tests need Docker.
verify: env fmt lint test test-integration doc

env:
	cargo --version
	rustc --version
	@command -v docker >/dev/null 2>&1 && docker --version || echo "WARN: docker not available; integration tests depend on it"

fmt:
	cargo fmt --check

lint:
	cargo clippy --all-targets -- -D warnings

test:
	cargo test

test-integration:
	cargo test -- --ignored

doc:
	cargo doc --no-deps
