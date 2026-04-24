import json
from pathlib import Path

import pytest

from helios_ai.models import FailureChain

FIXTURES = Path(__file__).parent / "fixtures"


@pytest.mark.parametrize("fixture", ["az_outage_chain.json", "region_outage_chain.json"])
def test_failure_chain_round_trip(fixture: str) -> None:
    raw = json.loads((FIXTURES / fixture).read_text())
    chain = FailureChain.model_validate(raw)
    assert chain.scenario
    assert len(chain.failures) >= 1
    for f in chain.failures:
        assert f.id
        assert f.kind
        assert f.reason
    assert chain.model_dump() == raw


def test_az_outage_fixture_details() -> None:
    raw = json.loads((FIXTURES / "az_outage_chain.json").read_text())
    chain = FailureChain.model_validate(raw)
    assert chain.scenario == "lose-us-east-1a"
    ids = [f.id for f in chain.failures]
    assert "aws_instance.web" in ids
    assert "aws_subnet.public_a" in ids


def test_glossary_mentions_every_model_variant() -> None:
    from helios_ai.glossary import AVAILABILITY_MODEL_GLOSSARY

    for variant in ("SingleAz", "MultiAz", "Regional", "GlobalEdge"):
        assert variant in AVAILABILITY_MODEL_GLOSSARY, variant
    # Must be large enough to benefit from prompt caching (~1024 tokens min).
    assert len(AVAILABILITY_MODEL_GLOSSARY) > 2000
