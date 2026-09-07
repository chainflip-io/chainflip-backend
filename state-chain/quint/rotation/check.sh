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
for f in types.qnt ceremony.qnt; do
  if quint typecheck "$f"; then echo "  ok $f"; else echo "  FAILED $f"; exit 1; fi
done

echo "== unit tests =="
quint test types.qnt
quint test ceremony.qnt --main=ceremony
