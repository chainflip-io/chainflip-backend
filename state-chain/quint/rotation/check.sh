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
quint test harness.qnt --main=fair

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
# W5 and W6 are listed and printed, but neither is a required positive: W5
# records a known livelock (see PF_NoPanics below) and W6 reads 0 on this
# instance (under STRONG at n=4 only one validator can ever be banned).
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

# The remaining validator-layer instances. quint exits non-zero on [violation]
# so the verdict gates the script; the required witnesses are checked explicitly
# because a witness that never fires is otherwise silent - a count of 0 means
# the behaviour the instance exists to exercise has become unreachable, the same
# kind of silent rot the negative controls below guard against.
run_instance() {
  local main="$1" invariants="$2" witnesses="$3" required="$4" samples="$5" out rc wname
  echo "== simulation (rotation, $main) =="
  set +e
  out=$(quint run harness.qnt --main="$main" --invariants $invariants \
    --witnesses $witnesses --max-steps=$MAIN_STEPS --max-samples="$samples" 2>&1)
  rc=$?
  set -e
  printf '%s\n' "$out" | grep -E '^\[(ok|violation)\]|witnessed in|--seed' | sed 's|^|    |'
  if [ $rc -ne 0 ]; then
    echo "FATAL: ${main} reported a violation (see the verdict above)." >&2
    exit 1
  fi
  for wname in $required; do
    if printf '%s\n' "$out" | grep -q "^${wname} was witnessed in 0 trace"; then
      echo "FATAL: required witness ${wname} was never witnessed on ${main}." >&2
      echo "       The instance no longer reaches the behaviour it exists to" >&2
      echo "       cover; this is a regression, not a passing check." >&2
      exit 1
    fi
  done
}

# `sol` is Uninitialised: activation completes on it with no key at all, so R6
# has to tolerate a complete chain whose activeKey is None (W9).
run_instance uninit \
  "R4_TransitionGating R5_NoNextKeyAfterAbort R6_KeyEpochAgreement \
   H3_NextKeyOnlyAfterActivation NoUnexpectedLogErrors" \
  "W9_UninitialisedChainCompletesWithoutKey" \
  "W9_UninitialisedChainCompletesWithoutKey" \
  $MAIN_SAMPLES

# STRONG = false: the keygen split, where honest nodes can be attributed and so
# banned (W8). Every safety invariant must still hold; only C3 at the ceremony
# layer is allowed to fail (see the NC1 negative control).
run_instance split \
  "R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating \
   R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation \
   H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation" \
  "W3_AbortAtSizeFloor W8_HonestBannedUnderSplit" \
  "W3_AbortAtSizeFloor W8_HonestBannedUnderSplit" \
  $MAIN_SAMPLES

# FAIR = true: deliveries precede blocks and every ceremony an honest set can
# finish does finish, so the two liveness properties become bounded invariants
# (L1 within K_BLOCKS, L2 no aborts) and PF_NoPanics is enforced here - unlike
# on `main`, the livelock that trips it is unreachable under the fair scheduler.
#
# 2000 samples, not $MAIN_SAMPLES. Under FAIR no trace aborts early, so every
# one of the 81 states is a live rotation and the instance runs at ~32 traces/s
# against ~6000/s on the other instances - 20000 samples costs ~10 minutes for
# no extra coverage, since W1 and W2 are witnessed in 100% of traces here.
FAIR_SAMPLES=2000
run_instance fair \
  "L1_Termination L2_Progress R7_NoAbortAfterActivation PF_NoPanics" \
  "W1_FullRotationWithHandover W2_RecoverFromKeygenFailure" \
  "W1_FullRotationWithHandover W2_RecoverFromKeygenFailure" \
  $FAIR_SAMPLES

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

# Exhaustive (Apalache) checks, opt-in because they are slow and shallow.
#
# Only `fair` is verified, and only at depth 1. The depth ladder on
# L1_Termination reads: depth 1 [ok] in 72 s, depth 2 dies with "Ran out of
# heap memory: Java heap space" after 181 s against Apalache's 4 GiB default.
# Verification on `main` is worse still (depth 1 alone takes 72 s and depth 2
# runs for hours), so the exhaustive layer buys almost nothing as the model is
# currently encoded; the simulation runs above are what actually cover it.
# Raising this depth needs the state space cut down first, not a bigger heap.
if [ "${1:-}" = "--verify" ]; then
  echo "== verify (rotation, fair) =="
  for inv in L1_Termination L2_Progress; do
    echo "  $inv at depth 1"
    quint verify harness.qnt --main=fair --invariant=$inv --max-steps=1 \
      | grep -E '^\[(ok|violation)\]' | sed 's|^|    |'
  done
fi
