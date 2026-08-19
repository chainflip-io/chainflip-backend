#!/usr/bin/env bash
# Run every Quint check for the multisig models.
#   ./check.sh          simulation only (fast, ~seconds)
#   ./check.sh --verify  add exhaustive Apalache checks (slow, ~11 minutes)
set -euo pipefail
cd "$(dirname "$0")"

command -v quint >/dev/null || { echo "quint not on PATH; see README.md"; exit 1; }

SIM_INVARIANTS=(
  "broadcast.qnt:L1_NoFalseBlame"
  "broadcast.qnt:L2_ValueAgreement"
  "broadcast.qnt:L3_HonestValuePreservation"
  "broadcast.qnt:L4_SafeDivergence"
  "broadcast.qnt:L6_VoteAgreement"
  "seam.qnt:SeamSound"
  "seam.qnt:SeamAgreementSound"
)
# Exhaustive checks, each ~2-3 minutes at n=4/f=1.
VERIFY_INVARIANTS=(
  "broadcast.qnt:L1_NoFalseBlame"
  "broadcast.qnt:L2_ValueAgreement"
  "broadcast.qnt:L6_VoteAgreement"
  "seam.qnt:SeamSound"
  "seam.qnt:SeamAgreementSound"
)

# The ten-stage keygen ceremony (keygen.qnt/harness.qnt) needs more steps than
# the echo-broadcast lemma's single one, so it gets its own step budget and
# loop rather than reusing --max-steps=1.
CEREMONY_STEPS=12

# `keygen.qnt` declares `SHARING`, `RECEIVING`, `FILTER_NON_RECEIVER_COMPLAINTS`
# and `ENFORCE_COEFF_LENGTH` as `const`, so it cannot be run directly (QNT500:
# Uninitialized const). Every ceremony check instead targets one of the
# `harness.qnt` modules via `--main`. Format: "file:main:invariant".
CEREMONY_INVARIANTS=(
  "harness.qnt:plain:K1_NoHonestBlamed"
  "harness.qnt:plain:K2_NoConflictingOutcome"
  "harness.qnt:plain:K3_AttributionProgress"
  "harness.qnt:plain:K5_Termination"
  "harness.qnt:plain:K6_KeyConsistency"
  "harness.qnt:handover:K4_HandoverNoFalseBlame"
)

# The ceremony properties verify exhaustively too, and cheaply (24-44s each
# measured). K2 and K6 only constrain states where a party reached Done,
# which simulation reaches in only ~0.05% of traces - too thin a witness rate
# to trust an `[ok]` from `quint run` alone, so these are checked with
# Apalache rather than relying on that sample. Format matches
# CEREMONY_INVARIANTS: "file:main:invariant".
CEREMONY_VERIFY_INVARIANTS=(
  "harness.qnt:plain:K1_NoHonestBlamed"
  "harness.qnt:plain:K2_NoConflictingOutcome"
  "harness.qnt:plain:K3_AttributionProgress"
  "harness.qnt:plain:K5_Termination"
  "harness.qnt:plain:K6_KeyConsistency"
  "harness.qnt:handover:K4_HandoverNoFalseBlame"
)

echo "== typecheck =="
# NOT `quint typecheck "$f" && echo ok`: under `set -e`, a command to the left
# of && is exempt from errexit, so a typecheck failure would print its error and
# the script would carry on and exit 0. A check script that exits 0 on failure
# is worse than no check script.
for f in types.qnt broadcast.qnt oracle.qnt seam.qnt keygen.qnt harness.qnt; do
  if quint typecheck "$f"; then
    echo "  ok $f"
  else
    echo "  FAILED $f"
    exit 1
  fi
done

echo "== unit tests =="
quint test broadcast.qnt
quint test oracle.qnt
quint test keygen.qnt

# Witnesses are not optional: an invariant that is never violated proves
# nothing if the interesting states were never reached. A witness at 0% means
# the run below it is vacuous.
WITNESSES="wAgreed wAttributed wUnattributed wDiverged"
CEREMONY_WITNESSES="wCeremonyDiverged wCeremonyDone wCeremonyBlamed"

echo "== simulation =="
for entry in "${SIM_INVARIANTS[@]}"; do
  echo "  ${entry}"
  quint run "${entry%%:*}" --invariant="${entry##*:}" --witnesses $WITNESSES \
    --max-steps=1 --max-samples=20000 \
    | grep -E '^\[(ok|violation)\]|witnessed in|Trace length' | sed 's|^|    |'
done

echo "== simulation (ceremony) =="
for entry in "${CEREMONY_INVARIANTS[@]}"; do
  IFS=':' read -r file main inv <<< "$entry"
  echo "  ${file}::${main}::${inv}"
  quint run "$file" --main="$main" --invariant="$inv" --witnesses $CEREMONY_WITNESSES \
    --max-steps="$CEREMONY_STEPS" --max-samples=20000 \
    | grep -E '^\[(ok|violation)\]|witnessed in|Trace length' | sed 's|^|    |'
done

if [[ "${1:-}" == "--verify" ]]; then
  echo "== exhaustive verification (slow) =="
  for entry in "${VERIFY_INVARIANTS[@]}"; do
    echo "  ${entry} ..."
    quint verify "${entry%%:*}" --invariant="${entry##*:}" --max-steps=1 \
      | grep -E '^\[(ok|violation)\]' | sed "s|^|  ${entry} |"
  done
  echo "== exhaustive verification (ceremony, slow) =="
  for entry in "${CEREMONY_VERIFY_INVARIANTS[@]}"; do
    IFS=':' read -r file main inv <<< "$entry"
    echo "  ${file}::${main}::${inv} ..."
    quint verify "$file" --main="$main" --invariant="$inv" --max-steps="$CEREMONY_STEPS" \
      | grep -E '^\[(ok|violation)\]' | sed "s|^|  ${file}::${main}::${inv} |"
  done
fi
