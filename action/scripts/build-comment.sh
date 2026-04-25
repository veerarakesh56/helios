#!/usr/bin/env bash
# Aggregate $ARTIFACT_DIR/*.json + $ARTIFACT_DIR/*.verify.txt into a single
# sticky-comment markdown body at $COMMENT_PATH.
#
# The marker `<!-- helios-action -->` in the body lets post-sticky.sh upsert
# instead of duplicating on each run.
#
# Inputs (env): ARTIFACT_DIR, COMMENT_PATH, ARTIFACT_NAME, RUN_ID, SERVER_URL, REPOSITORY
# Output (GHA step): body-path = $COMMENT_PATH
set -euo pipefail

artifact_url="$SERVER_URL/$REPOSITORY/actions/runs/$RUN_ID#artifacts"

{
  echo "<!-- helios-action -->"
  echo "## Helios PR analysis"
  echo
  echo "Ran \`helios inspect\` over $(ls "$ARTIFACT_DIR"/*.json 2>/dev/null | wc -l | tr -d ' ') scenario(s)."
  echo "Inspect JSON artifacts: [\`$ARTIFACT_NAME\`]($artifact_url) (download and drop into the [web viewer](../../tree/main/web))."
  echo
} > "$COMMENT_PATH"

shopt -s nullglob
for inspect_json in "$ARTIFACT_DIR"/*.json; do
  stem="$(basename "$inspect_json" .json)"
  failure_count=$(jq '.chain.failures | length' "$inspect_json")
  scenario_label=$(jq -r '.chain.scenario' "$inspect_json")

  {
    echo "<details>"
    if [[ "$failure_count" -eq 0 ]]; then
      echo "<summary><strong>$stem</strong> — ✅ no failures (\`$scenario_label\`)</summary>"
    else
      echo "<summary><strong>$stem</strong> — ❌ $failure_count failure(s) (\`$scenario_label\`)</summary>"
    fi
    echo
    if [[ "$failure_count" -gt 0 ]]; then
      echo
      echo "| Resource | Kind | Reason |"
      echo "|---|---|---|"
      jq -r '.chain.failures[:3][] | "| `\(.id)` | \(.kind) | \(.reason) |"' "$inspect_json"
      if [[ "$failure_count" -gt 3 ]]; then
        echo
        echo "_…and $((failure_count - 3)) more in the artifact._"
      fi
    fi

    verify_txt="$ARTIFACT_DIR/$stem.verify.txt"
    if [[ -f "$verify_txt" ]]; then
      echo
      echo "**With fix applied (\`fixes/$stem.json\`):**"
      echo
      echo '```'
      cat "$verify_txt"
      echo '```'
    fi
    echo
    echo "</details>"
    echo
  } >> "$COMMENT_PATH"
done
shopt -u nullglob

echo "body-path=$COMMENT_PATH" >> "$GITHUB_OUTPUT"
echo "comment body $(wc -c < "$COMMENT_PATH") bytes:"
cat "$COMMENT_PATH"
