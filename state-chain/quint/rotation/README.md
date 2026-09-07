# Quint model of authority rotation stages

Formal model of the rotation orchestration in `cf-validator`, the per-chain
key rotation status in `cf-threshold-signature`, the multi-chain combinator
in `runtime/src/chainflip/cons_key_rotator.rs`, and vault activation in
`cf-vaults`. See `DESIGN.md` for scope, properties and the oracle contract.

## Setup

```bash
npm install -g @informalsystems/quint    # 0.32.0 or later
quint --version
```

Apalache (for `quint verify`) and the Rust evaluator are downloaded into
`~/.quint` on first use. Apalache needs a JDK.

## Running

```bash
./check.sh            # typecheck, tests, simulation, negative controls (~1 min)
./check.sh --verify   # add exhaustive Apalache checks (budget ~15 min)
```

Every module except `types.qnt` declares `const`s, so checks target an
instance in `harness.qnt` via `--main`:

```bash
quint run harness.qnt --main=main --invariant=PF_NoPanics --max-steps=40
quint test harness.qnt --main=main
```

## Status

(filled in by Task 8)

## Gotchas

- Never write `setOfMaps(D, C).oneOf()`; Apalache rejects it, `quint run` does not.
- Draw `nondet` values from `powerset()` and decode them; never constrain a
  free draw inside `all { }`.
- `Option` is not in the standard library; `types.qnt` defines it.
- Without `--main`, quint picks the module named after the file, which is
  wrong for every multi-module file here.
- `fail` is a built-in; do not name an action `fail`.

## Known gaps

(filled in by Task 8)
