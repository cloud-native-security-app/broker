.PHONY: verify env fmt lint test test-integration doc compose-check

# Verification sequence for the broker crate. Integration tests need Docker.
verify: env compose-check fmt lint test test-integration doc

env:
	cargo --version
	rustc --version
	@command -v docker >/dev/null 2>&1 && docker --version || echo "WARN: docker not available; integration tests and compose-check depend on it"

compose-check:
	docker compose config >/dev/null

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
