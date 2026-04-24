"""Thin argparse shell. `python -m helios_ai explain` reads FailureChain
JSON on stdin and writes a markdown narrative on stdout.

If HELIOS_AI_MOCK=1 is set, a canned fake client is used instead of the
real Anthropic SDK — used by the Rust end-to-end smoke test and anyone
running the CLI without an API key.
"""

from __future__ import annotations

import argparse
import os
import sys
from typing import Any

from .explain import explain
from .models import FailureChain


def _build_client() -> Any:
    if os.environ.get("HELIOS_AI_MOCK") == "1":
        from ._mock import MockAnthropic

        return MockAnthropic()

    import anthropic  # lazy — tests that mock _build_client don't need SDK

    api_key = os.environ.get("ANTHROPIC_API_KEY")
    if not api_key:
        raise SystemExit("ANTHROPIC_API_KEY not set")
    return anthropic.Anthropic(api_key=api_key)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="helios-ai")
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("explain", help="Read FailureChain JSON on stdin, write narrative on stdout.")
    args = parser.parse_args(argv)

    if args.cmd == "explain":
        raw = sys.stdin.read()
        chain = FailureChain.model_validate_json(raw)
        client = _build_client()
        sys.stdout.write(explain(chain, client=client))
        sys.stdout.write("\n")
        return 0

    return 2


if __name__ == "__main__":
    raise SystemExit(main())
