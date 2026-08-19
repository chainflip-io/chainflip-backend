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
quint typecheck broadcast.qnt
quint run broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1 --max-samples=20000
quint verify broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1
```

`quint run` samples and is fast (~30k traces/s); it finds bugs but proves
nothing. `quint verify` is exhaustive via Apalache and slow. Use `run` while
developing, `verify` before believing a result.

## Gotchas

- Never write `setOfMaps(D, C).oneOf()`. Apalache rejects it ("Trying to expand
  a set of functions"). `quint run` accepts it, so this only bites at verify
  time. Use `Set[record].powerset().oneOf()` with well-formedness constraints.
- `Option` is not in the standard library.
- There is no `quint eval`; use `quint repl -r file.qnt::module`.

## Status

Verified exhaustively at n=4 with 1 Byzantine party (`quint verify`):

| Property | Meaning | Time |
| --- | --- | --- |
| L1 NoFalseBlame | an honest node is never blamed | ~74 s |
| L2 ValueAgreement | honest nodes never agree on different values | ~79 s |
| SeamSound | the oracle's blame clause matches concrete `verify_broadcasts` | ~72 s |
| SeamAgreementSound | the oracle's agreement clause matches it too | ~78 s |

Checked by simulation only: L3, L4.

`SeamSound` alone is a weaker check than it looks: it is per-party, so its only
falsifiable content is the no-blame clause. `SeamAgreementSound` is what covers
agreement. Mutating `verify` so the agreed map depends on the observing party is
caught by the second and missed entirely by the first.

**Not a property of this protocol:** full outcome agreement. A Byzantine party
can equivocate in round 1 and tie-break in round 2, leaving some honest parties
`Agreed` and others failing. This is a liveness degradation, not a safety
violation — see the spec for the counterexample.
