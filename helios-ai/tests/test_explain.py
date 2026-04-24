from __future__ import annotations

from pathlib import Path

import pytest

from helios_ai.explain import explain
from helios_ai.models import FailureChain

FIXTURES = Path(__file__).parent / "fixtures"


@pytest.mark.parametrize("fixture", ["az_outage_chain.json", "region_outage_chain.json"])
def test_explain_matches_snapshot(fake_client, snapshot, fixture: str) -> None:
    chain = FailureChain.model_validate_json((FIXTURES / fixture).read_text())
    narrative = explain(chain, client=fake_client)
    assert narrative == snapshot


def test_explain_sends_cache_markers(fake_client) -> None:
    chain = FailureChain(scenario="dummy", failures=[])
    explain(chain, client=fake_client)
    call = fake_client.messages.calls[0]
    system = call["system"]
    assert isinstance(system, list)
    markers = [b for b in system if b.get("cache_control") == {"type": "ephemeral"}]
    assert len(markers) >= 2, "expected 2 cache breakpoints (system + glossary)"
    assert call["model"] == "claude-opus-4-7"


def test_explain_passes_failure_chain_json_on_user_turn(fake_client) -> None:
    chain = FailureChain(scenario="test-xyz", failures=[])
    explain(chain, client=fake_client)
    call = fake_client.messages.calls[0]
    user_content = call["messages"][0]["content"]
    assert '"scenario"' in user_content
    assert '"test-xyz"' in user_content
