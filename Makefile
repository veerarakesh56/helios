# Helios convenience targets.
#
# `make demo` is what the README GIF records: simulate -> explain -> verify
# end-to-end against the bundled three-tier-webapp + az-outage fixture.
# Set HELIOS_AI_MOCK=1 for a no-network run; otherwise set ANTHROPIC_API_KEY.

.PHONY: demo test fmt check doc python-check web-check

# Detect Python venv interpreter path per platform.
ifeq ($(OS),Windows_NT)
HELIOS_AI_PYTHON ?= helios-ai/.venv/Scripts/python.exe
else
HELIOS_AI_PYTHON ?= helios-ai/.venv/bin/python
endif

export HELIOS_AI_PYTHON

demo:
	cargo build --bin helios
	cargo run -q -p helios-cli -- simulate fixtures/three-tier-webapp \
		--scenario fixtures/scenarios/az-outage.yaml --json \
		| cargo run -q -p helios-cli -- explain
	cargo run -q -p helios-cli -- verify fixtures/three-tier-webapp \
		--scenario fixtures/scenarios/az-outage.yaml \
		--fix fixes/az-outage.json

test:
	cargo test --workspace
	cd helios-ai && $(HELIOS_AI_PYTHON) -m pytest
	cd web && npm test

fmt:
	cargo fmt --all
	cd helios-ai && $(HELIOS_AI_PYTHON) -m ruff format .

check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
	cd helios-ai && $(HELIOS_AI_PYTHON) -m ruff check .
	cd helios-ai && $(HELIOS_AI_PYTHON) -m pytest
	cd web && npm run build
	cd web && npm test

doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --open

python-check:
	cd helios-ai && $(HELIOS_AI_PYTHON) -m ruff check .
	cd helios-ai && $(HELIOS_AI_PYTHON) -m pytest

web-check:
	cd web && npm run build
	cd web && npm test
