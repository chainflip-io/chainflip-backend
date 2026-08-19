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
