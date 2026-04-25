# The AI boundary

Helios is a Rust + Z3 simulator with a Claude-powered explanation shell. The
single most important architectural rule is this:

> **AI never produces a safety verdict. The engine does.**

This document explains what that means in practice, why we drew the line
exactly here, and how `helios verify` enforces the rule on every fix proposal.

## Two layers, one source of truth

Helios runs in two stages:

1. **Engine (Rust + Z3).** Parses Terraform JSON into a typed resource graph,
   compiles `graph + scenario` into SMT constraints, asks Z3 for a model that
   violates an availability invariant, and emits a `FailureChain`. The engine
   has zero AI in the path. Same input, same output, every time.
2. **Shell (Python + Claude).** Reads the `FailureChain` JSON over stdio and
   produces (a) a human-readable narration of the failure, and (b) a
   structured `FixProposal` -- a JSON document of `set_attr` edits to apply
   to the graph.

The shell never decides whether something is safe. It explains what the
engine already proved, and it suggests changes whose safety the engine then
re-checks.

## How `helios verify` closes the loop

Every fix proposal goes through this loop:

```
graph + scenario -> engine.simulate() -> FailureChain (pre)
            |
            +-> Claude propose_fix(chain, attrs_snapshot)
                          |
                          v
                    FixProposal { edits: [SetAttr ...] }
                          |
                          v
        engine.apply_fix(graph, fix) -> graph'
                          |
                          v
        engine.simulate(graph', scenario) -> FailureChain (post)
                          |
                          v
        VerifyReport { resolved, new_failures, remaining }
```

`helios verify` exits non-zero if any post-fix failure remains. The fix is
"safe" only because the engine re-ran the same Z3 procedure that found the
original failure -- not because Claude said so. If Claude hallucinated an
edit that does not actually fix the chain, the verify step catches it and
the CLI fails the run.

This is the answer to the inevitable question: *how do you know Claude is
not hallucinating?* We do not trust the proposal. We re-verify it.

## Where AI is and is not used

| Layer | Uses AI? | Why |
|---|---|---|
| Terraform JSON parsing | No | Deterministic; hallucinations would mean wrong graphs |
| Availability models | No | Authored and peer-reviewed by humans, version-controlled |
| Z3 constraint encoding | No | The encoding is the proof obligation |
| Constraint solving | No | Z3. Correctness is the entire point |
| Counter-example narration | Yes (Claude Opus) | Translates an SMT model into English |
| Fix proposal | Yes (Claude Opus) | Suggests `set_attr` edits, then **re-verified by engine** |
| Plain-English scenario parser (v0.2) | Yes (Claude Sonnet) | "what if us-east-1 goes down" -> scenario YAML |

## Prior art: differential testing as a verification harness

Helios is not the first system to put a deterministic core behind an
LLM-generated proposal. The pattern shows up in the formal-methods world
under the label *differential testing*: produce a candidate by any means
(grammar fuzzers, learned models, hand-written templates), then run a
trusted oracle on the candidate.

The cleanest reference is `cedar-policy/cedar` -- AWS's open-source policy
engine. Cedar ships a Rust evaluator and an SMT-LIB symbolic encoder, and
the differential testing harness checks that every concrete evaluation of a
policy agrees with the SMT result for the same input. The candidate is the
policy author's intent; the SMT solver is the oracle.

Helios uses the same shape:

| Element | Cedar | Helios |
|---|---|---|
| Candidate | Hand-authored policy | Claude-proposed `FixProposal` |
| Oracle | SMT-LIB symbolic encoder | Z3 + availability-model constraints |
| Verdict | Concrete vs symbolic agree | Post-fix `FailureChain` is empty |

The lesson is the same: do not ship verdicts that came out of the
candidate-producer. Ship verdicts that came out of the oracle.

## What this rules out

Some things Helios deliberately will *not* do, because they would smear
the boundary:

- **No "AI auto-merge" of fixes.** A `FixProposal` is a CI artifact, not a
  PR commit. Humans (and the engine) gate the merge.
- **No LLM scoring of failure chains.** The engine returns a `FailureChain`
  with concrete failed-resource ids. Claude narrates them; it does not
  re-rank or filter them.
- **No tool-use to call AWS APIs and "check" things at runtime.** All facts
  enter the system through the graph builder. If a fact is missing, the
  fix is to add it to the model -- not to ask Claude to look it up.
- **No fine-tuned model for "safety classification".** A statistical
  classifier cannot meet the standard a Z3 proof meets.

## What this rules *in*

Within the boundary, the AI shell is allowed to be expressive:

- Long-form explanations of *why* a chain failed, citing the availability
  model in plain English.
- Multiple fix proposals per failure -- the engine verifies each one and
  the CLI ranks by minimal diff size.
- Plain-English scenario authoring: "us-east-1 partial outage for 45
  minutes" -> the scenario YAML the engine accepts.
- Postmortem mining: ingest an incident write-up, propose regression
  scenarios for the engine to run forever after.

All of these are candidate-generators. None of them is a verdict.

## Why this matters for production use

Differential testing is the only honest answer to "is this AI output
safe?". Without an oracle, every safety claim is a marketing claim. With an
oracle, the safety claim is a property of the oracle, not of the model
that produced the candidate.

If you are evaluating Helios for use in your CI pipeline, the question to
ask is not "what model do you use?" -- the model is a moving target -- but
"what does the oracle prove?". For Helios v0.1, the oracle proves: *the
post-fix graph contains no resources whose `failed` boolean is forced true
by the scenario constraints.* That is a small statement. It is also a true
statement, every time.

That smallness is the point. We promise less than a marketing-driven AI
product would promise. We deliver exactly what we promise.

## Further reading

- Source of truth for the encoding: `crates/helios-engine/src/smt.rs`.
- Source of truth for the verification loop: `crates/helios-engine/src/verify.rs`.
- Cedar differential testing: `https://github.com/cedar-policy/cedar`.
- The `propose_fix` schema lives in `helios-ai/src/helios_ai/models.py` and
  is mirrored 1:1 from the Rust `FixProposal` struct.
