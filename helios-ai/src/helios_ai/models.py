"""Pydantic mirrors of the Rust `helios_engine::report` types.

Field names and shapes MUST match `FailureChain` / `FailedResource` in
`helios/crates/helios-engine/src/report.rs`. When the Rust struct changes,
this file changes in the same PR.
"""

from __future__ import annotations

from pydantic import BaseModel, ConfigDict


class FailedResource(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: str
    kind: str
    reason: str


class FailureChain(BaseModel):
    model_config = ConfigDict(extra="forbid")

    scenario: str
    failures: list[FailedResource]
