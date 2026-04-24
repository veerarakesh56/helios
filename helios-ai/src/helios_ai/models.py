"""Pydantic mirrors of the Rust `helios_engine::report` + `fix` types.

Field names and shapes MUST match `FailureChain` / `FailedResource` in
`helios/crates/helios-engine/src/report.rs` and `FixProposal` / `FixEdit`
in `helios/crates/helios-engine/src/fix.rs`. When the Rust structs change,
this file changes in the same PR.
"""

from __future__ import annotations

from typing import Any, Literal

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


class FixEdit(BaseModel):
    """A single structural edit on a resource's Terraform attrs.

    v0.1 only supports `op: "set_attr"`. `add_resource` / `remove_resource`
    land in v0.2 alongside the richer graph write surface in helios-engine.
    """

    model_config = ConfigDict(extra="forbid")

    op: Literal["set_attr"]
    resource_id: str
    key: str
    value: Any


class FixProposal(BaseModel):
    """A candidate remediation — the engine re-simulates to verify it."""

    model_config = ConfigDict(extra="forbid")

    scenario_name: str
    explanation: str
    edits: list[FixEdit]
