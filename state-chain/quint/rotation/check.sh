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
for f in types.qnt ceremony.qnt chain.qnt validator.qnt harness.qnt; do
  if quint typecheck "$f"; then echo "  ok $f"; else echo "  FAILED $f"; exit 1; fi
done

echo "== unit tests =="
quint test types.qnt
quint test ceremony.qnt --main=ceremony
quint test chain.qnt
quint test harness.qnt --main=main

echo "== simulation (ceremony) =="
CEREMONY_STEPS=6
CEREMONY_SAMPLES=20000
CEREMONY_INVARIANTS="C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound"
CEREMONY_WITNESSES="wResolvedSuccess wResolvedFailure wByzantinePunished"
quint run harness.qnt --main=ceremonyStrong --invariants $CEREMONY_INVARIANTS \
  --witnesses $CEREMONY_WITNESSES --max-steps=$CEREMONY_STEPS --max-samples=$CEREMONY_SAMPLES \
  | grep -E '^\[(ok|violation)\]|witnessed in' | sed 's|^|    |'

echo "== simulation (rotation, main) =="
# Depth 80, not 40: a full rotation needs ~20 deliveries plus the blocks that
# consume them, and at depth 40 only 0.15% of traces complete one (1.2% at 80).
# The compound witnesses (W1/W2/W4) need that headroom to fire at all.
MAIN_STEPS=80
MAIN_SAMPLES=20000
MAIN_INVARIANTS="R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating \
  R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation H2_UtxoAlwaysHandsOver \
  H3_NextKeyOnlyAfterActivation H4_ConsSoundness NoUnexpectedLogErrors"
# W5 and W6 are deliberately absent: W5 records a known livelock (see PF_NoPanics
# below) and W6 reads 0 on this instance, so neither is a required positive.
MAIN_WITNESSES="W1_FullRotationWithHandover W2_RecoverFromKeygenFailure W3_AbortAtSizeFloor \
  W4_CompleteDespiteSafeMode W5_HandoverVerificationLivelock W6_AbortSharingUnavailable \
  W7_HandoverRetry"
quint run harness.qnt --main=main --invariants $MAIN_INVARIANTS \
  --witnesses $MAIN_WITNESSES --max-steps=$MAIN_STEPS --max-samples=$MAIN_SAMPLES \
  | grep -E '^\[(ok|violation)\]|witnessed in' | sed 's|^|    |'

# PF_NoPanics is a KNOWN [violation] on `main`: the handover-verification
# livelock (W5) reaches the handover_invalid_state assertion. Its verdict is a
# finding recorded in README.md, not a regression, so it is reported here but
# deliberately does not gate the script. Do not "fix" the model to make it pass.
echo "== panic freedom (rotation, main) - REPORTED, NOT ENFORCED =="
set +e
pf_output=$(quint run harness.qnt --main=main --invariant=PF_NoPanics \
  --max-steps=$MAIN_STEPS --max-samples=$MAIN_SAMPLES 2>&1)
set -e
printf '%s\n' "$pf_output" | grep -E '^\[(ok|violation)\]|--seed' | sed 's|^|    |'

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
  "harness.qnt:main:NC3_EveryRetryBans_MustFailHere"
)

echo "== negative controls (MUST_VIOLATE) =="
for entry in "${MUST_VIOLATE[@]}"; do
  IFS=':' read -r file main inv <<< "$entry"
  echo "  ${file}::${main}::${inv}"
  set +e
  # main-instance controls need the rotation depth, not the ceremony depth.
  if [ "$main" = "main" ]; then steps=$MAIN_STEPS; samples=$MAIN_SAMPLES
  else steps=$CEREMONY_STEPS; samples=$CEREMONY_SAMPLES; fi
  output=$(quint run "$file" --main="$main" --invariant="$inv" \
    --max-steps=$steps --max-samples=$samples 2>&1)
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
