# helios-ai

Python AI shell for [Helios](../README.md). Reads `FailureChain` JSON from the Rust engine on stdin and writes a human-readable markdown narrative on stdout via Claude.

## Dev

    uv sync
    uv run pytest
    uv run ruff check

## Use

    helios simulate ./infra --scenario scenarios/az-outage.yaml --json \
      | ANTHROPIC_API_KEY=... uv run python -m helios_ai explain

Or go through the Rust wrapper once `helios explain` lands:

    helios simulate ./infra --scenario scenarios/az-outage.yaml --json | helios explain
