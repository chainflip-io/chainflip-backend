#!/usr/bin/env bash
# Run every Quint check for the multisig models.
#   ./check.sh          simulation only (fast, ~seconds)
#   ./check.sh --verify  add exhaustive Apalache checks (slow, ~15 minutes)
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

echo "== typecheck =="
# NOT `quint typecheck "$f" && echo ok`: under `set -e`, a command to the left
# of && is exempt from errexit, so a typecheck failure would print its error and
# the script would carry on and exit 0. A check script that exits 0 on failure
# is worse than no check script.
for f in types.qnt broadcast.qnt oracle.qnt seam.qnt; do
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

# Witnesses are not optional: an invariant that is never violated proves
# nothing if the interesting states were never reached. A witness at 0% means
# the run below it is vacuous.
WITNESSES="wAgreed wAttributed wUnattributed wDiverged"

echo "== simulation =="
for entry in "${SIM_INVARIANTS[@]}"; do
  echo "  ${entry}"
  quint run "${entry%%:*}" --invariant="${entry##*:}" --witnesses $WITNESSES \
    --max-steps=1 --max-samples=20000 \
    | grep -E '^\[(ok|violation)\]|witnessed in|Trace length' | sed 's|^|    |'
done

if [[ "${1:-}" == "--verify" ]]; then
  echo "== exhaustive verification (slow) =="
  for entry in "${VERIFY_INVARIANTS[@]}"; do
    echo "  ${entry} ..."
    quint verify "${entry%%:*}" --invariant="${entry##*:}" --max-steps=1 \
      | grep -E '^\[(ok|violation)\]' | sed "s|^|  ${entry} |"
  done
fi
