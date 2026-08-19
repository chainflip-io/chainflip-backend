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
./check.sh --verify   # add exhaustive Apalache checks (~11 minutes)
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

| Property | Meaning | Time |
| --- | --- | --- |
| K1 NoHonestBlamed (`--main=plain`) | no honest party is blamed by any honest party | ~41 s |
| K2 NoConflictingOutcome (`--main=plain`) | no two honest parties finish with different keys | ~35 s |
| K3 AttributionProgress (`--main=plain`) | a non-empty blame set always contains a Byzantine party | ~40 s |
| K5 Termination (`--main=plain`) | no honest party is stuck `Running` once it reaches `Finished` | ~25 s |
| K6 KeyConsistency (`--main=plain`) | a ceremony never finalises with a wrong-length commitment in play | ~38 s |
| K4 HandoverNoFalseBlame (`--main=handover`) | K1 re-checked under the handover split (`SHARING={2,3,4}`, `RECEIVING={1,2,3}`) | ~44 s |

`wCeremonyDone` (an honest party reaching `Done`) is thin under simulation —
around 0.03-0.07% of sampled traces, i.e. only ~10-15 traces out of 20000 —
because it is the last of ten stages and depends on every nondet draw in the
step lining up. K2 and K6 only constrain states where a party reached `Done`,
so a `quint run` `[ok]` on them would rest on that thin a sample. That is
exactly why all six ceremony properties are checked with `quint verify`
instead of relying on simulation: Apalache explores the full state space
regardless of how rarely a random walk reaches `Done`, so the K2/K6 result
does not depend on witness coverage at all.

Total verify time for all eleven checks above: ~10m22s (budget: ~11 minutes).

### Checked by simulation only (`quint run` — samples, does not prove)

- **L3, L4** (`broadcast.qnt`).

Everything else — the full lemma layer and the full ceremony layer, including
handover — is exhaustively verified per the tables above. Simulation is still
run for all of them too (`./check.sh`, no `--verify`) as a fast pre-verify
smoke check; witness coverage for the ceremony simulation runs
(`wCeremonyDiverged`, `wCeremonyDone`, `wCeremonyBlamed`, 20000 samples,
12 steps, trace length max=13 as required) is `wCeremonyDiverged` and
`wCeremonyBlamed` consistently well over 85%, `wCeremonyDone` 0.03-0.07% and
never 0.00% in any run performed.

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
