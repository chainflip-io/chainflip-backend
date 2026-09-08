#!/usr/bin/env bash
# Run every Quint check for the rotation model.
#   ./check.sh          typecheck, tests, simulation, negative controls (~3 min)
#   ./check.sh --verify add exhaustive Apalache checks (~18 min in total)
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

# Every simulated instance goes through this helper. quint exits non-zero on
# [violation] so the verdict gates the script; the required witnesses are
# checked explicitly because a witness that never fires is otherwise silent -
# a count of 0 means the behaviour the instance exists to exercise has become
# unreachable, the same kind of silent rot the negative controls below guard
# against. Witnesses that are listed but NOT passed as required (W5, W6) are
# printed only; their counts are findings recorded in README.md.
run_instance() {
  local main="$1" invariants="$2" witnesses="$3" required="$4" steps="$5" samples="$6" out rc wname
  echo "== simulation ($main) =="
  set +e
  out=$(quint run harness.qnt --main="$main" --invariants $invariants \
    --witnesses $witnesses --max-steps="$steps" --max-samples="$samples" 2>&1)
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

CEREMONY_STEPS=6
CEREMONY_SAMPLES=20000
run_instance ceremonyStrong \
  "C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound" \
  "wResolvedSuccess wResolvedFailure wByzantinePunished" \
  "wResolvedSuccess wResolvedFailure wByzantinePunished" \
  $CEREMONY_STEPS $CEREMONY_SAMPLES

# Depth 80, not 40: a full rotation needs ~20 deliveries plus the blocks that
# consume them, and at depth 40 only 0.15% of traces complete one (1.2% at 80).
# The compound witnesses (W1/W2/W4) need that headroom to fire at all.
MAIN_STEPS=80
MAIN_SAMPLES=20000
# W5 and W6 are listed and printed, but neither is a required positive: W5
# records a known livelock (see PF_NoPanics below) and W6 reads 0 on this
# instance (under STRONG at n=4 only one validator can ever be banned).
run_instance main \
  "R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating \
   R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation \
   H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation H4_ConsSoundness \
   NoUnexpectedLogErrors" \
  "W1_FullRotationWithHandover W2_RecoverFromKeygenFailure W3_AbortAtSizeFloor \
   W4_CompleteDespiteSafeMode W5_HandoverVerificationLivelock \
   W6_AbortSharingUnavailable W7_HandoverRetry" \
  "W1_FullRotationWithHandover W2_RecoverFromKeygenFailure \
   W4_CompleteDespiteSafeMode W7_HandoverRetry" \
  $MAIN_STEPS $MAIN_SAMPLES

# PF_NoPanics is a KNOWN [violation] on `main`: the handover-verification
# livelock (W5) reaches the handover_invalid_state assertion. Its verdict is a
# finding recorded in README.md, not a regression, so it is reported here but
# deliberately does not gate the script. Do not "fix" the model to make it pass.
echo "== panic freedom (main) - REPORTED, NOT ENFORCED =="
set +e
pf_output=$(quint run harness.qnt --main=main --invariant=PF_NoPanics \
  --witnesses W5_HandoverVerificationLivelock \
  --max-steps=$MAIN_STEPS --max-samples=$MAIN_SAMPLES 2>&1)
set -e
printf '%s\n' "$pf_output" | grep -E '^\[(ok|violation)\]|witnessed in|--seed' | sed 's|^|    |'

# `sol` is Uninitialised: activation completes on it with no key at all, so R6
# has to tolerate a complete chain whose activeKey is None (W9).
run_instance uninit \
  "R4_TransitionGating R5_NoNextKeyAfterAbort R6_KeyEpochAgreement \
   H3_NextKeyOnlyAfterActivation NoUnexpectedLogErrors" \
  "W9_UninitialisedChainCompletesWithoutKey" \
  "W9_UninitialisedChainCompletesWithoutKey" \
  $MAIN_STEPS $MAIN_SAMPLES

# STRONG = false: the keygen split, where honest nodes can be attributed and so
# banned (W8). Every safety invariant must still hold; only C3 at the ceremony
# layer is allowed to fail (see the NC1 negative control).
run_instance split \
  "R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating \
   R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation \
   H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation" \
  "W3_AbortAtSizeFloor W8_HonestBannedUnderSplit" \
  "W3_AbortAtSizeFloor W8_HonestBannedUnderSplit" \
  $MAIN_STEPS $MAIN_SAMPLES

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
  $MAIN_STEPS $FAIR_SAMPLES

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
# Depths are per property, not global: each entry below sits at the deepest
# rung that returned a verdict inside the budget (see README "Tractability").
# The ceremony layer verifies at depth 6 in seconds; the validator layer is
# exhaustive only at depth 1 on the two-chain `main`/`fair` instances and
# depth 2 on the one-chain `main1`, neither of which reaches a completed
# rotation. Raising those needs the state space cut down, not a bigger heap:
# depth 2 on `main` was killed at 8138 s, and depth 3 on `main1` times out.
if [ "${1:-}" = "--verify" ]; then
  echo "== exhaustive verification (slow, ~15 min; runs are sequential: Apalache holds port 8822) =="
  # macOS ships no `timeout`; perl's alarm is the portable stand-in. A capped
  # run kills quint but not its Apalache JVM child - run `pkill -f apalache`
  # after an interrupted --verify pass.
  if command -v timeout >/dev/null; then CAP=(timeout 900)
  elif command -v gtimeout >/dev/null; then CAP=(gtimeout 900)
  else CAP=(perl -e 'alarm shift; exec @ARGV' 900); fi

  # Three outcomes, as in the MUST_VIOLATE loop, and only the first is a pass.
  # The output is captured rather than piped straight into grep: quint verify
  # exits non-zero on a real counterexample, pipefail propagates that, and a
  # `... | grep ... || echo NO VERDICT` fallback would both mislabel the
  # violation and hand the compound statement an exit status of 0 - so the
  # script would exit 0 on a genuine Apalache counterexample.
  VERIFY_FAILED=0
  verify() { # main invariant depth [extra flags...]
    local main="$1" inv="$2" depth="$3"; shift 3
    echo "  ${main}::${inv} depth ${depth} ..."
    local out result
    set +e
    out=$(JVM_ARGS="-Xmx8g" "${CAP[@]}" quint verify harness.qnt --main="$main" \
      --invariant="$inv" --max-steps="$depth" "$@" 2>&1)
    set -e
    result="$(printf '%s\n' "$out" | grep -E '^\[(ok|violation)\]' | head -n 1 || true)"
    if printf '%s\n' "$result" | grep -q '^\[ok\]'; then
      echo "  ${main}::${inv} ${result}"
    elif printf '%s\n' "$result" | grep -q '^\[violation\]'; then
      echo "  ${main}::${inv} ${result}"
      VERIFY_FAILED=1
    else
      echo "  ${main}::${inv} NO VERDICT (timeout or error)"
      VERIFY_FAILED=1
    fi
  }

  # The ceremony layer is a one-shot model: once `result` is set every action in
  # `step` is disabled, and without no-deadlocks Apalache reports that intended
  # terminal state as a deadlock. The config is a FILE path - the inline-JSON
  # form of --apalache-config is silently ignored.
  for inv in C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound; do
    verify ceremonyStrong $inv 6 --apalache-config=apalache-no-deadlocks.json
  done
  # Validator layer: only these fit. Depth 1 on the two-chain instance and depth 2
  # on the one-chain instance; neither reaches a completed rotation (README "Status").
  for inv in L1_Termination L2_Progress; do verify fair $inv 1; done
  # `fun-arrays` is what makes depth 2 reachable at all on main1; H4_ConsSoundness
  # is trivially true on one chain and is deliberately not verified here.
  for inv in R2_SizeFloor R4_TransitionGating H3_NextKeyOnlyAfterActivation; do
    verify main1 $inv 2 --apalache-config=apalache-fun-arrays.json
  done

  if [[ "${VERIFY_FAILED:-0}" == "1" ]]; then
    echo "FATAL: exhaustive verification reported a violation or produced no verdict" >&2
    exit 1
  fi
fi
