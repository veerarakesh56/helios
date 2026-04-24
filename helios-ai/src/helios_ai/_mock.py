"""Minimal fake Anthropic client for tests and HELIOS_AI_MOCK smoke paths.

Shipped inside the package (not only in tests/) so the Rust end-to-end
test can trigger it via `HELIOS_AI_MOCK=1 python -m helios_ai explain`
without the tests/ directory being on sys.path.
"""

from __future__ import annotations

import json as _json
from dataclasses import dataclass, field
from typing import Any


@dataclass
class _TextBlock:
    type: str
    text: str


@dataclass
class _Usage:
    input_tokens: int = 0
    output_tokens: int = 0
    cache_creation_input_tokens: int = 0
    cache_read_input_tokens: int = 0


@dataclass
class _Message:
    content: list[_TextBlock]
    usage: _Usage
    stop_reason: str = "end_turn"


class _Messages:
    def __init__(self) -> None:
        self.calls: list[dict[str, Any]] = []

    def create(self, **kwargs: Any) -> _Message:
        self.calls.append(kwargs)
        scenario = "unknown"
        for m in kwargs.get("messages", []):
            content = m.get("content", "")
            if isinstance(content, str) and '"scenario"' in content:
                try:
                    scenario = _json.loads(content).get("scenario", scenario)
                except _json.JSONDecodeError:
                    pass
        text = (
            f"# Failure narrative for {scenario}\n\n"
            "Claude would explain the chain here. (mocked)"
        )
        return _Message(
            content=[_TextBlock(type="text", text=text)],
            usage=_Usage(input_tokens=100, output_tokens=20),
        )


@dataclass
class MockAnthropic:
    """Ducks the attributes of `anthropic.Anthropic` that `explain()` uses."""

    messages: _Messages = field(default_factory=_Messages)

    def __init__(self, *_: Any, **__: Any) -> None:
        self.messages = _Messages()
