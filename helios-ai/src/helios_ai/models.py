"""Pydantic mirrors of the Rust `helios_engine` types.

Field names and shapes MUST match `FailureChain` / `FailedResource` in
`helios/crates/helios-engine/src/report.rs`, `FixProposal` / `FixEdit`
in `helios/crates/helios-engine/src/fix.rs`, and `InspectDoc` family in
`helios/crates/helios-engine/src/inspect.rs`. When the Rust structs change,
this file changes in the same PR.
"""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field


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


class NodeDoc(BaseModel):
    """One resource in the inspect graph."""

    model_config = ConfigDict(extra="forbid")

    id: str
    kind: str
    attrs: Any


class DepDoc(BaseModel):
    """Tagged dependency edge weight: kind ∈ {Contains, MemberOf}, via = attr name."""

    model_config = ConfigDict(extra="forbid")

    kind: Literal["Contains", "MemberOf"]
    via: str


class EdgeDoc(BaseModel):
    """One directed edge: source/target Terraform addresses + dep weight."""

    # Rust emits `from`; alias the Python attribute to avoid the keyword clash.
    model_config = ConfigDict(extra="forbid", populate_by_name=True)

    from_: str = Field(alias="from")
    to: str
    dep: DepDoc


class GraphDoc(BaseModel):
    """Flat node + edge lists keyed on Terraform addresses."""

    model_config = ConfigDict(extra="forbid")

    nodes: list[NodeDoc]
    edges: list[EdgeDoc]


class InspectDoc(BaseModel):
    """Top-level `helios inspect` output: scenario + graph + resulting chain."""

    model_config = ConfigDict(extra="forbid")

    scenario: str
    graph: GraphDoc
    chain: FailureChain
