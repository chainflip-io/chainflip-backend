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
./check.sh --verify   # add exhaustive Apalache checks (~12 minutes)
quint typecheck broadcast.qnt
quint run broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1 --max-samples=20000
quint verify broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1
quint test harness.qnt --main=plain   # includes doneReachableTest - see Status below
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

Times are from the most recent `./check.sh --verify` run.

**Echo-broadcast lemma layer** (`broadcast.qnt`) and its correspondence to the
oracle abstraction the ceremony model builds on (`seam.qnt`):

| Property | Meaning | Time |
| --- | --- | --- |
| L1 NoFalseBlame | an honest node is never blamed | ~78 s |
| L2 ValueAgreement | honest nodes never agree on different values | ~79 s |
| L6 VoteAgreement | honest nodes never disagree on a quorum-vote result | ~83 s |
| SeamSound | the oracle's blame clause matches concrete `verify_broadcasts` | ~78 s |
| SeamAgreementSound | the oracle's agreement clause matches it too | ~82 s |

`SeamSound` alone is a weaker check than it looks: it is per-party, so its only
falsifiable content is the no-blame clause. `SeamAgreementSound` is what covers
agreement. Mutating `verify` so the agreed map depends on the observing party is
caught by the second and missed entirely by the first.

**Ten-stage keygen ceremony layer** (`keygen.qnt` via `harness.qnt`, `--main`
routing as shown):

All six ceremony checks below (and both negative controls in the section
below) are bounded at `--max-steps=12`. The stage machine needs at most 10
transitions to reach `Finished` from `PubkeyShares0` (11 states including
init), so this bound sits comfortably above the deepest reachable trace — it
is not a restriction on what the checks can see.

| Property | Meaning | Time |
| --- | --- | --- |
| K1 NoHonestBlamed (`--main=plain`) | no honest party is blamed by any honest party | ~73 s |
| K2 NoConflictingOutcome (`--main=plain`) | no two honest parties finish with different keys | ~41 s |
| K3 AttributionProgress (`--main=plain`) | a non-empty blame set always contains a Byzantine party | ~57 s |
| K5 Termination (`--main=plain`) | no honest party is stuck `Running` once it reaches `Finished` | ~27 s |
| K6 KeyConsistency (`--main=plain`) | a ceremony never finalises with a wrong-length commitment in play | ~54 s |
| K4 HandoverNoFalseBlame (`--main=handover`) | K1 re-checked under the handover split (`SHARING={2,3,4}`, `RECEIVING={1,2,3}`) | ~51 s |

K2 holds partly by construction: `step` (`keygen.qnt`) draws exactly one
agreed key per step, so two honest parties are never even offered different
agreed maps to disagree on. Its falsifiable content is therefore "the
encoding preserves L2" — L2 ValueAgreement is what `broadcast.qnt` proves
independently, over the concrete broadcast primitive rather than the
encoding shortcut.

**`Done`-reachability is proven deterministically, not by sampling.** Since
the blame-response-completeness fix (F3) wired `Done` behind agreement at all
four verify stages simultaneously, a uniformly random walk over the
adversarial choice space reaches `Done` in only about 1 in 45000 traces
(~0.002%) — correct model behaviour (a genuine consequence of closing a real
gap), but the wrong instrument for a reachability claim: too rare for
`quint run`'s `CEREMONY_SAMPLES` (20000) to reliably witness, and no sample
size that keeps `./check.sh` fast changes that. Reachability is established
instead by `doneReachableTest` (`harness.qnt`, `plain` module): a
deterministic `run` test that drives `stepWith` — the parameterised core
`step` draws its random arguments for — through the happy path (agreement at
every verify stage, no bad shares, no complaints, no blame, honest
coefficient lengths) and asserts every honest party ends in `Done`. It has no
`nondet` of its own, runs in milliseconds, and is checked by `quint test`,
not `quint run`. Combined with the exhaustive `quint verify` of K2/K6 below
(which explores the full state space regardless of how rarely a random walk
reaches `Done`), `wCeremonyDone`'s simulation count is not required to be
non-zero — see the Status section below.

Total verify time for all eleven checks above: ~11m43s (budget: ~12 minutes).

### Checked by simulation only (`quint run` — samples, does not prove)

- **L3, L4** (`broadcast.qnt`).

Everything else — the full lemma layer and the full ceremony layer, including
handover — is exhaustively verified per the tables above. Simulation is still
run for all of them too (`./check.sh`, no `--verify`) as a fast pre-verify
smoke check; witness coverage for the ceremony simulation runs
(`wCeremonyDiverged`, `wCeremonyDone`, `wCeremonyBlamed`, 20000 samples,
12 steps, trace length max=13 as required) is `wCeremonyDiverged` and
`wCeremonyBlamed` consistently well over 85% — both are required-positive,
and doing real coverage work. `wCeremonyDone` is not: at this sample size it
routinely reads 0 traces, and that is expected, not a vacuousness signal —
see the note on its rate above. `Done`-reachability is established by
`doneReachableTest` and the exhaustive K2/K6 verification, not by this
simulation pass.

### Permanent negative controls

Two `harness.qnt` modules exist solely to prove the model can see the bugs the
Rust already fixes. Both currently report `[violation]`, as required — if
either ever reports `[ok]`, the model has lost the power to detect that bug
class and should not be trusted. Both are run automatically by `./check.sh`
(the `MUST_VIOLATE` section) with the exit condition inverted: a `[violation]`
is a pass, and an `[ok]` makes `check.sh` fail loudly rather than pass
silently:

| Control | Command | Result |
| --- | --- | --- |
| `handoverUnfixed` / `K4_MustFailHere` | `quint run harness.qnt --main=handoverUnfixed --invariant=K4_MustFailHere --max-steps=12 --max-samples=50000` | `[violation]` — an honest sharer is wrongly attributed after a non-receiving Byzantine party (party 4) complains about it; the forced reveal at a non-receiver's index fails stage-9 verification |
| `plainNoCoeffCheck` / `K6_MustFailHere` | `quint run harness.qnt --main=plainNoCoeffCheck --invariant=K6_MustFailHere --max-steps=12 --max-samples=50000` | `[violation]` — reproduces review finding #1: a Byzantine party commits to `KEY_THRESHOLD + 2` coefficients and the ceremony still reaches `Done` for an honest party |

The handover split (`SHARING = {2,3,4}`, `RECEIVING = {1,2,3}`, Byzantine party
4 a sharer that is *not* a receiver) is load-bearing, not arbitrary: with
`RECEIVING = {3,4}` (making the Byzantine party a receiver too) the same
control reports `[ok]` — the bug becomes unreachable and the control proves
nothing. This was confirmed directly, not just asserted.

**Not a property of this protocol:** full outcome agreement. A Byzantine party
can equivocate in round 1 and tie-break in round 2, leaving some honest parties
`Agreed` and others failing. This is a liveness degradation, not a safety
violation — see the spec for the counterexample.
