#!/usr/bin/env bash
# Loop over every scenario YAML matching $SCENARIOS_GLOB. For each:
#   - run `helios inspect` and dump JSON to $ARTIFACT_DIR/<stem>.json
#   - if $FIXES_DIR/<stem>.json exists, run `helios verify` and capture
#     a per-scenario summary line into $ARTIFACT_DIR/<stem>.verify.txt
#
# Inputs (env): HELIOS_BIN, SCENARIOS_GLOB, FIXES_DIR, TERRAFORM_JSON, ARTIFACT_DIR
# Output (GHA step): artifact-dir = $ARTIFACT_DIR
set -euo pipefail

mkdir -p "$ARTIFACT_DIR"

# Expand the glob portably. Globbing inside `for` requires no quotes.
shopt -s nullglob
matches=( $SCENARIOS_GLOB )
shopt -u nullglob

if [[ ${#matches[@]} -eq 0 ]]; then
  echo "::error::no scenarios matched glob '$SCENARIOS_GLOB'"
  exit 1
fi

for scenario in "${matches[@]}"; do
  stem="$(basename "$scenario" .yaml)"
  echo "::group::scenario $stem"

  inspect_out="$ARTIFACT_DIR/$stem.json"
  if ! "$HELIOS_BIN" inspect "$TERRAFORM_JSON" --scenario "$scenario" > "$inspect_out"; then
    echo "::error::helios inspect failed for $stem"
    exit 1
  fi
  echo "wrote $inspect_out ($(wc -c < "$inspect_out") bytes)"

  fix_path="$FIXES_DIR/$stem.json"
  if [[ -f "$fix_path" ]]; then
    verify_out="$ARTIFACT_DIR/$stem.verify.txt"
    # `verify` exits non-zero if any failures remain — capture but don't abort.
    set +e
    "$HELIOS_BIN" verify "$TERRAFORM_JSON" --scenario "$scenario" --fix "$fix_path" \
      > "$verify_out" 2>&1
    rc=$?
    set -e
    echo "wrote $verify_out (verify rc=$rc)"
  fi

  echo "::endgroup::"
done

echo "artifact-dir=$ARTIFACT_DIR" >> "$GITHUB_OUTPUT"
