# Quint models of the multisig ceremony

Formal models of the keygen ceremony in `engine/multisig/src/client/`,
checking Byzantine attribution correctness. See
`docs/superpowers/specs/2026-08-19-frost-quint-model-design.md` for the design.

## Setup

```bash
npm install -g @informalsystems/quint
# nvm users: the binary may not be on a non-interactive shell's PATH
export PATH="$(npm root -g)/../bin:$PATH"
```

Apalache (for `quint verify`) and the Rust evaluator (for `quint run`) are
downloaded automatically into `~/.quint` on first use. Apalache needs a JDK.

## Running

```bash
./check.sh            # everything, with the configurations that are known to fit
./check.sh --verify   # add exhaustive Apalache checks (~7-15 minutes)
quint typecheck broadcast.qnt
quint run broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1 --max-samples=20000
quint verify broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1
```

`quint run` samples and is fast (~30k traces/s); it finds bugs but proves
nothing. `quint verify` is exhaustive via Apalache and slow. Use `run` while
developing, `verify` before believing a result.

`keygen.qnt` (the ten-stage ceremony model) declares `SHARING`, `RECEIVING`,
`FILTER_NON_RECEIVER_COMPLAINTS` and `ENFORCE_COEFF_LENGTH` as `const`, so it
cannot be run or verified directly. Every ceremony check instead targets one
of the concrete instantiations in `harness.qnt` via `--main`:

```bash
quint run harness.qnt --main=plain --invariant=K1_NoHonestBlamed --max-steps=12 --max-samples=20000
quint run harness.qnt --main=handover --invariant=K4_HandoverNoFalseBlame --max-steps=12 --max-samples=20000
```

`harness.qnt` also holds two permanent negative controls (`handoverUnfixed`,
`plainNoCoeffCheck`) — see Status below.

## Gotchas

- Never write `setOfMaps(D, C).oneOf()`. Apalache rejects it ("Trying to expand
  a set of functions"). `quint run` accepts it, so this only bites at verify
  time. Use `Set[record].powerset().oneOf()` with well-formedness constraints.
- `Option` is not in the standard library.
- There is no `quint eval`; use `quint repl -r file.qnt::module`.
- Without `--main`, quint infers the module from the filename - wrong for a
  multi-module file like `harness.qnt`. Every harness check must pass `--main`
  explicitly.

## Status

### Exhaustively verified (`quint verify`, n=4, 1 Byzantine party)

These are properties of the echo-broadcast primitive (`broadcast.qnt`) and its
correspondence to the oracle abstraction the ceremony model builds on
(`seam.qnt`). Times are from the most recent `./check.sh --verify` run.

| Property | Meaning | Time |
| --- | --- | --- |
| L1 NoFalseBlame | an honest node is never blamed | ~77 s |
| L2 ValueAgreement | honest nodes never agree on different values | ~83 s |
| L6 VoteAgreement | honest nodes never disagree on a quorum-vote result | ~79 s |
| SeamSound | the oracle's blame clause matches concrete `verify_broadcasts` | ~78 s |
| SeamAgreementSound | the oracle's agreement clause matches it too | ~84 s |

Total verify time for the suite above: ~7m17s (budget: 15 minutes).

`SeamSound` alone is a weaker check than it looks: it is per-party, so its only
falsifiable content is the no-blame clause. `SeamAgreementSound` is what covers
agreement. Mutating `verify` so the agreed map depends on the observing party is
caught by the second and missed entirely by the first.

### Checked by simulation only (`quint run` — samples, does not prove)

- **L3, L4** (`broadcast.qnt`).
- **K1 NoHonestBlamed, K2 NoConflictingOutcome, K3 AttributionProgress,
  K5 Termination, K6 KeyConsistency** — the ten-stage keygen ceremony
  (`keygen.qnt` via `harness.qnt --main=plain`), n=4, 1 Byzantine party,
  12 steps, 20000 samples.
- **K4 HandoverNoFalseBlame** — K1 re-checked under the handover
  configuration (`harness.qnt --main=handover`: `SHARING = {2,3,4}`,
  `RECEIVING = {1,2,3}`), same bounds.

None of these were run through `quint verify`. Exhaustive verification of the
full ten-stage ceremony was out of scope for this plan (see "Deferred" in the
design spec) — its state space is much larger than the echo-broadcast lemma's
(seven nondet draws per step vs. the lemma's few), and no configuration was
found or attempted that fits an Apalache run in a reasonable time. Treat K1-K6
and K4 as "no counterexample found in the traces sampled," not as proven.

Witness coverage for the ceremony checks (`wCeremonyDiverged`, `wCeremonyDone`,
`wCeremonyBlamed`, 20000 samples, 12 steps, trace length max=13 as required):
`wCeremonyDiverged` and `wCeremonyBlamed` are both consistently well over
85%. `wCeremonyDone` — an honest party reaching `Done` — is thin and has been
falling as nondet dimensions accumulated across tasks: ~0.55% after Task 10,
0.12-0.16% after Task 11, and now **0.03-0.06%** (observed across the `plain`
and `handover` configurations; confirmed nonzero at 100k samples too, ~0.05%).
It is never 0.00% in any run performed, so K2 and K6 (the only invariants that
bite exclusively on `Done`) are not vacuous, but the coverage is sparse enough
that a future task narrowing the state space further should re-check this
before trusting a green K2/K6 result.

### Permanent negative controls

Two `harness.qnt` modules exist solely to prove the model can see the bugs the
Rust already fixes. Both currently report `[violation]`, as required — if
either ever reports `[ok]`, the model has lost the power to detect that bug
class and should not be trusted:

| Control | Command | Result |
| --- | --- | --- |
| `handoverUnfixed` / `K4_MustFailHere` | non-receiver complaint filter (`FILTER_NON_RECEIVER_COMPLAINTS`) switched off under the handover split | `[violation]` — an honest sharer is wrongly attributed after a non-receiving Byzantine party (party 4) complains about it; the forced reveal at a non-receiver's index fails stage-9 verification |
| `plainNoCoeffCheck` / `K6_MustFailHere` | coefficient-length check (`ENFORCE_COEFF_LENGTH`) switched off | `[violation]` — reproduces review finding #1: a Byzantine party commits to `KEY_THRESHOLD + 2` coefficients and the ceremony still reaches `Done` for an honest party |

The handover split (`SHARING = {2,3,4}`, `RECEIVING = {1,2,3}`, Byzantine party
4 a sharer that is *not* a receiver) is load-bearing, not arbitrary: with
`RECEIVING = {3,4}` (making the Byzantine party a receiver too) the same
control reports `[ok]` — the bug becomes unreachable and the control proves
nothing. This was confirmed directly, not just asserted.

**Not a property of this protocol:** full outcome agreement. A Byzantine party
can equivocate in round 1 and tie-break in round 2, leaving some honest parties
`Agreed` and others failing. This is a liveness degradation, not a safety
violation — see the spec for the counterexample.
