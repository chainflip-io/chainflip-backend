#!/bin/bash
# Decides which optional checks a pull request runs. A check runs if its `ci:` label is on the PR
# or if the PR touches a path the check guards.
# Reads the pull_request event from GITHUB_EVENT_PATH and writes `<check>=true|false` to GITHUB_OUTPUT.

set -euo pipefail

EVENT=${GITHUB_EVENT_PATH:?}
OUTPUT=${GITHUB_OUTPUT:-/dev/stdout}
SUMMARY=${GITHUB_STEP_SUMMARY:-/dev/stderr}

pr_number=$(jq -r .pull_request.number "$EVENT")
labels=$(jq -r '.pull_request.labels[].name' "$EVENT")

files=""
for page in $(seq 1 30); do
  batch=$(curl -fsSL --retry 3 -H "Authorization: Bearer $GH_TOKEN" \
    "$GITHUB_API_URL/repos/$GITHUB_REPOSITORY/pulls/$pr_number/files?per_page=100&page=$page" \
    | jq -r '.[] | .filename, (.previous_filename // empty)')
  [[ -z "$batch" ]] && break
  files+="$batch"$'\n'
done

# Changes to the selection itself force every check, so the PR that changes it exercises it.
selector_paths='^(ci/scripts/select_optional_pr_checks\.sh|\.github/workflows/ci-pr-optional\.yml)$'
# The generated chaintypes mirror runtime metadata, so a change there means on-chain types changed.
upgrade_test_paths="(^|/)migrations(/|\.rs$)|^bouncer/generated/chaintypes/"
upgrade_test_paths+="|^state-chain/runtime/src/runtime_apis/|^state-chain/custom-rpc/"
upgrade_test_paths+="|^engine-upgrade-utils/|^bouncer/(commands|shared)/[a-z_]*upgrade[a-z_]*\.ts$"
upgrade_test_paths+="|^\.github/workflows/upgrade-test\.yml$|$selector_paths"
full_bouncer_tests=$(grep -oE "from 'tests/[a-z_]+'" bouncer/tests/full_bouncer.test.ts \
  | sed -E "s/.*tests\/([a-z_]+)'/\1/" | paste -sd'|' -)
full_bouncer_paths="^bouncer/full_bouncer\.sh$|^bouncer/tests/(full_bouncer|gaslimit_ccm)\.test\.ts$"
full_bouncer_paths+="|^bouncer/tests/($full_bouncer_tests)\.ts$|$selector_paths"

{
  echo "### Optional checks"
  echo
  echo "| Check | Runs | Why |"
  echo "|---|---|---|"
} >>"$SUMMARY"

select_check() {
  local check=$1 paths=$2 reasons="" hits
  if grep -qxF "ci:$check" <<<"$labels"; then
    reasons+="label \`ci:$check\`; "
  fi
  hits=$(grep -E "$paths" <<<"$files" | head -3 | paste -sd, - || true)
  if [[ -n "$hits" ]]; then
    reasons+="touches $hits; "
  fi
  local run=false
  [[ -n "$reasons" ]] && run=true
  echo "$check=$run" >>"$OUTPUT"
  echo "| \`$check\` | $run | ${reasons%; } |" >>"$SUMMARY"
}

select_check full-bouncer "$full_bouncer_paths"
select_check upgrade-test "$upgrade_test_paths"

echo -e "\nAdd a \`ci:full-bouncer\` or \`ci:upgrade-test\` label to run a check that wasn't selected." >>"$SUMMARY"
