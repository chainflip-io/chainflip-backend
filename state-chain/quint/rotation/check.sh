#!/usr/bin/env bash
# Run every Quint check for the rotation model.
#   ./check.sh          simulation only
#   ./check.sh --verify add exhaustive Apalache checks
set -euo pipefail
cd "$(dirname "$0")"

command -v quint >/dev/null || { echo "quint not on PATH; see README.md"; exit 1; }

echo "== typecheck =="
# `if quint typecheck; then` rather than `&&`: under set -e a failing command
# left of && does not abort, and a check script that exits 0 on failure is
# worse than none.
for f in types.qnt ceremony.qnt harness.qnt; do
  if quint typecheck "$f"; then echo "  ok $f"; else echo "  FAILED $f"; exit 1; fi
done

echo "== unit tests =="
quint test types.qnt
quint test ceremony.qnt --main=ceremony

echo "== simulation (ceremony) =="
CEREMONY_STEPS=6
CEREMONY_SAMPLES=20000
CEREMONY_INVARIANTS="C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound"
CEREMONY_WITNESSES="wResolvedSuccess wResolvedFailure wByzantinePunished"
quint run harness.qnt --main=ceremonyStrong --invariants $CEREMONY_INVARIANTS \
  --witnesses $CEREMONY_WITNESSES --max-steps=$CEREMONY_STEPS --max-samples=$CEREMONY_SAMPLES \
  | grep -E '^\[(ok|violation)\]|witnessed in' | sed 's|^|    |'

# Negative controls: the model's proof that it can still see the bugs the Rust
# already fixes. Both of these MUST report [violation] - an [ok] means the
# model has lost the power to detect that bug class and check.sh must fail
# loudly rather than pass silently. The exit condition is therefore INVERTED
# relative to every other check in this script, so `set -e` is suspended
# around each `quint run` call (which itself exits non-zero on [violation])
# and the pass/fail decision is made explicitly below instead.
MUST_VIOLATE=(
  "harness.qnt:ceremonySplit:NC1_HonestNeverOffender_MustFailHere"
  "harness.qnt:ceremonyOutage:NC2_DropNeverEmpties_MustFailHere"
)

echo "== negative controls (MUST_VIOLATE) =="
for entry in "${MUST_VIOLATE[@]}"; do
  IFS=':' read -r file main inv <<< "$entry"
  echo "  ${file}::${main}::${inv}"
  set +e
  output=$(quint run "$file" --main="$main" --invariant="$inv" \
    --max-steps=$CEREMONY_STEPS --max-samples=$CEREMONY_SAMPLES 2>&1)
  set -e
  result="$(printf '%s\n' "$output" | grep -E '^\[(ok|violation)\]' || true)"
  echo "    ${result}"
  # Three outcomes, not two. Collapsing the last two into "reported [ok]" sends
  # whoever reads the FATAL hunting for a weakened model when the real cause may
  # be a renamed invariant, a bad --main, or a toolchain error.
  if printf '%s\n' "$result" | grep -q '^\[violation\]'; then
    : # the control fired, as it must
  elif printf '%s\n' "$result" | grep -q '^\[ok\]'; then
    echo "FATAL: negative control ${file}::${main}::${inv} reported [ok]." >&2
    echo "       This negative control has gone INERT - the model can no" >&2
    echo "       longer detect the bug class it exists to catch. Do not" >&2
    echo "       treat this as a passing check." >&2
    exit 1
  else
    echo "FATAL: negative control ${file}::${main}::${inv} produced no verdict." >&2
    echo "       quint printed neither [ok] nor [violation]. Raw output:" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
done
