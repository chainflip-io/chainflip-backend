# Quint model of authority rotation stages

**Date:** 2026-09-07
**Status:** design approved, not yet implemented
**Scope:** `state-chain/pallets/cf-validator` (rotation phases),
`state-chain/pallets/cf-threshold-signature` (key rotator, ceremony response
voting), `state-chain/pallets/cf-vaults` (vault activator),
`state-chain/runtime/src/chainflip/cons_key_rotator.rs` (multi-chain
combinator).
**Linear:** PRO-3120, under Formal Protocol Verification, second item on
the project's candidate list. Sequel to the keygen model (PRO-3084).

## Motivation

The keygen model (PRO-3084, `engine/multisig/quint/`) verifies that a single
ceremony attributes blame correctly. It stops at the point where each node
reports an outcome to the State Chain. Everything after that is orchestration:
the State Chain collects reports per chain, resolves an outcome, retries or
bans, runs handover, activates keys on every chain, and finally rotates the
session. That orchestration is a six-phase state machine in `cf-validator`
polling a cons-list of four independent eleven-variant state machines in
`cf-threshold-signature`, each delegating activation to `cf-vaults`.

The existing tests are five pallet unit tests and nine integration tests, each
a single hand-written scenario. None explores interleavings across chains:
one chain completing handover while another fails, verification signing
failing with an unattributable offender set, safe mode flipping mid-phase.
Those interleavings are where a rotation can wedge, and a wedged rotation has
no governance exit once it is past `Idle` (`force_rotation` requires `Idle`).

A stateful proptest would not need a rewrite (the validator mock already
exposes a settable key rotator), but it can only drive the validator pallet
against one mocked rotator. It cannot see the cons composition, the per-chain
status machines, timeouts, activation, or the next-epoch key write, and it
cannot state liveness. Quint can. A trace-replay conformance layer against the
mock is a possible follow-up (see Deferred), not a substitute.

## What the model must cover

Boundary "B" from the scoping discussion: validator orchestration **plus** the
per-chain key rotation status machines and the cons combinator, over a small
set of abstract chains. Broadcast barriers and which key signs in-flight
transactions are out (that slice belongs to the broadcast model).

The rotation phases (`cf-validator/src/lib.rs`, `RotationPhase`):

```text
Idle → KeygensInProgress → KeyHandoversInProgress → ActivatingKeys
     → NewKeysActivated → SessionRotating → Idle
```

with abort edges back to `Idle` from the first three, and retry edges:
keygen failure re-runs the auction excluding banned nodes and restarts keygen;
handover failure restarts keygen if any offender is a candidate, otherwise bans
and retries handover with a fresh sharing set.

The per-chain key rotation status (`cf-threshold-signature/src/lib.rs`,
`KeyRotationStatus`), all eleven variants:

```text
AwaitingKeygen → AwaitingKeygenVerification → KeygenVerificationComplete
  → AwaitingKeyHandover → AwaitingKeyHandoverVerification → KeyHandoverComplete
  → AwaitingActivationSignatures → Complete
  | Failed { offenders } | KeyHandoverFailed { new_public_key, offenders }
```

Handover is required only on UTXO chains (`key_handover_is_required`). Other
chains go straight to `KeyHandoverComplete`, as does any chain with no active
epoch key yet.

### Threshold arithmetic

Copied from `cf-utilities`: `t(n) = (2n − 1) / 3` (integer division),
success threshold `t + 1`, failure threshold `n − t`. At n=4: t=2, success=3,
failure=2, tolerated Byzantine coalition 1.

## Architecture

Five modules in this directory (`state-chain/quint/rotation/`), plus
`check.sh`, `README.md`, and this design document. No Rust changes.

| Module | Contents |
| --- | --- |
| `types.qnt` | validator ids and honest/Byzantine split, chain ids with UTXO and vault tags, abstract keys, threshold arithmetic |
| `ceremony.qnt` | per-ceremony response status: reports, timeout, `resolve_keygen_outcome`, the failure-threshold drop from `progress_rotation`; verified standalone |
| `chain.qnt` | one `KeyRotationStatus` machine per chain over the ceremony oracle, epoch key map, vault activation, the cons combinator as a pure function |
| `validator.qnt` | the six rotation phases, abstract auction, ban bookkeeping, size floor, sharing-set selection, safe mode, force rotation, broadcasts-pending gate, two session boundaries, epoch and authority sets |
| `harness.qnt` | instances, invariants, negative controls, witnesses, deterministic run tests |

The layering mirrors the keygen model: a concrete lower layer discharges an
explicit oracle contract, and the upper layers are verified over the oracle so
that vote sequences never enter their state space.

### The abstraction boundary

The chain and validator layers never see votes. A ceremony is a single
nondeterministic outcome:

```text
Ceremony(candidates) returns one of:
  Success(k)      — only if every candidate reported k
  Failure(off)    — off ⊆ candidates; may be empty
```

with one optional clause used only by the progress property:

```text
  StrongHonesty   — if every honest candidate reported the same outcome,
                    then off ∩ honest = ∅
```

`StrongHonesty` is deliberately stronger than what the keygen model proved.
Its README records that a Byzantine party can equivocate so that some honest
parties finish `Agreed` and others fail. In that split the State Chain's
resolve rule bans the honest minority (their success or failure votes lose to
the super-majority), and `terminate_rotation` then reports and slashes them.
The ceremony layer is therefore checked against both assumptions: `C3` holds
under `StrongHonesty`, and negative control `NC1` shows it failing under the
split. Upper-layer safety properties use only the first two clauses.

A seam invariant over the concrete ceremony layer (`SeamSound`) states that
every resolved ceremony satisfies the contract.

### Faithful vs. abstract

| Rust | Model |
| --- | --- |
| `RotationPhase`, six variants | faithful |
| `RotationState { primary_candidates, banned, bond, new_epoch_index }` | faithful minus `bond` |
| `resolve_auction_iteratively` | abstract: winners are a nondeterministic subset of qualified unbanned bidders within `[min_size, max_size]`, or `AuctionFailed` |
| `select_sharing_participants` (seeded shuffle) | same set semantics, nondeterministic choice instead of the shuffle |
| `MaxAuthoritySetContractionPercentage` floor | faithful, as a constant; `Percent * u32` rounds to nearest (ties down), so 70% of 4 is 3 |
| `KeyRotationStatus`, eleven variants | faithful |
| `ResponseStatus` and `resolve_keygen_outcome` | faithful in `ceremony.qnt`; oracle above it |
| `KeygenResponseTimeout` block clock | abstract timeout action, no clock |
| keygen and handover verification signing | abstract result `Ok` or `Err(off)`, `off ⊆ participants`, possibly empty (signing drops offenders above half the candidates) |
| `handover_key_matches` | key equality; a fresh keygen key is unique per ceremony, a successful handover reproduces the old key |
| `VaultActivator`, multi-vault for EVM | one vault per chain; outcomes `Normal`, `NotRequired`, `TxFailed` (awaiting governance), `NotInitialised` (complete, no key) |
| `Keys`, `CurrentKeyEpoch`, `set_key_for_epoch` | faithful |
| `ConsKeyRotator::status` | faithful, pure function over the chain statuses |
| `pallet_session` boundaries | two deterministic steps: `NewKeysActivated → SessionRotating`, then epoch transition and `Idle` |
| safe mode `authority_rotation_enabled` | boolean environment toggle |
| `RotationBroadcastsPending` | boolean environment toggle |
| `force_rotation` | environment action guarded by `Idle` and safe mode |
| offence reporting, slashing | not modelled; the slashing consequence of `NC1` is noted in the README |
| epoch expiry, historical authorities, delegation snapshots | not modelled |

### Scheduling

A step is either one environment or adversary action, or one block. A block
runs the hooks in the runtime's `construct_runtime` order: the validator hook
first (it polls the chain statuses as left by the previous block), then the
session boundary if `should_end_session` holds, then the vault hooks, then the
threshold-signature hooks for every chain (resolve ceremonies whose candidates
have all reported or whose timeout fired, deliver pending signing results).
So a chain-layer change made in block `n` is first seen by the validator in
block `n + 1`. The model checker explores all interleavings of actions and
blocks.

If exhaustive verification does not fit the depth budget, the first lever is
depth compression: a block step may also deliver any pending verification,
activation, or signature results in the same step. This shortens traces at the
cost of branching and must not change the reachable set of phase transitions.

## Adversary and environment model

**Byzantine validators.** At most the tolerated coalition (one at n=4). At
the State Chain level a Byzantine candidate may report any key, any blame set
drawn from the participants, or nothing. Reports are validated as in
`handle_key_ceremony_report!`: one report per remaining candidate, blames
outside the participant set are dropped. A Byzantine sharing participant may
cause a handover to fail (expressed through the oracle outcome).

**Honest validators.** Reports are derived from the oracle outcome. `Agreed(k)`:
every honest candidate reports `Success(k)`. `Failed(B)` with
`B ⊆ Byzantine ∩ participants`: every honest candidate reports `Failure(B)`.
`Split`: some honest report `Success(k)` and the rest `Failure(B)`; enabled
only in the weak-assumption instance used by `NC1`. Honest candidates report
before the timeout fires.

**Environment.** Timeouts, verification signing results, activation outcomes,
activation signatures becoming ready, governance unblocking a failed
activation, safe mode toggles, broadcasts-pending toggles, the epoch becoming
due, `force_rotation`, and the bidder set changing between auction
resolutions within a fixed universe of validators.

## Properties

Identifiers are stable and used verbatim in `harness.qnt` and the README.

### Ceremony layer (`ceremony.qnt`)

| Id | Property |
| --- | --- |
| C1 AcceptanceUnanimity | `Success(k)` only if every candidate reported `k` |
| C2 OffendersAreParticipants | `Failure(off)` implies `off ⊆ candidates` |
| C3 HonestNeverOffender | under `StrongHonesty`, `off ∩ honest = ∅` |
| SeamSound | every resolved ceremony satisfies the oracle contract |

Negative controls (must report a violation):

| Id | What it shows |
| --- | --- |
| NC1 HonestNeverOffender_MustFailHere | under the split, an honest minority is banned |
| NC2 EveryFailureBansSomeone_MustFailHere | offenders drop to empty at the failure threshold |

### Chain layer (`chain.qnt`)

| Id | Property |
| --- | --- |
| H1 VerifiedImpliesUnanimousAndSigned | `KeygenVerificationComplete { k }` implies an earlier `Success(k)` and a signing `Ok` |
| H2 HandoverPreservesKey | on a UTXO chain with old key `k0`, `KeyHandoverComplete { k }` implies `k = k0` |
| H3 NextKeyOnlyAfterActivation | `Keys[epoch + 1]` is set only after `activate_keys` ran in this rotation |
| H4 ConsSoundness | merged `Failed(off)` implies some chain is in `Failed` or `KeyHandoverFailed`, and `off` is the union of their offenders |

Panic freedom, one invariant per assertion site in `key_rotator.rs` and
`lib.rs`:

| Id | Assertion |
| --- | --- |
| PF1 | `keygen` is never requested while a chain's status is `Pending`, nor with no candidates |
| PF2 | `key_handover` is never requested while `Pending`, nor from a status other than `KeygenVerificationComplete`, `KeyHandoverFailed`, `KeyHandoverComplete` |
| PF3 | a chain in `AwaitingKeyHandover` always has an active epoch key |
| PF4 | `activate_keys` is only called from `KeyHandoverComplete` |

`H4` may fail by design: the combinator returns `Failed(∅)` for any mixed
pair of `Ready` statuses. If that pair is reachable, the result is a finding
about the combinator, not a modelling error.

### Validator layer (`validator.qnt`)

| Id | Property |
| --- | --- |
| R1 BannedNeverAuthority | at the epoch transition, `new_authorities ∩ banned = ∅` |
| R2 SizeFloor | at the epoch transition, `|new_authorities| ≥ max(min_size, (1 − contraction) × |old_authorities|)` |
| R3 SharingSetValidity | when handover is requested with sharing set `S`: `S ⊆ current_authorities \ banned`, `|S| ≥ success_threshold(|current_authorities|)`, and every UTXO chain has an old key |
| R4 TransitionGating | `SessionRotating` implies every chain is `Complete`, and the queued authorities equal the final `authority_candidates()` |
| R5 NoNextKeyAfterAbort | after an abort, no chain has `CurrentKeyEpoch = epoch + 1` |
| R6 KeyEpochAgreement | all chains with an active vault agree on `CurrentKeyEpoch` |

Negative control (must report a violation):

| Id | What it shows |
| --- | --- |
| NC3 EveryRetryBans_MustFailHere | a keygen or handover retry can leave `banned` unchanged, so an unbounded retry loop with the same set is reachable |

### Liveness

Stated as temporal formulas in the spec; checked as described under
Verification.

| Id | Property | Fairness assumed |
| --- | --- | --- |
| L1 Termination | `phase ≠ Idle` leads to `phase = Idle` | every open ceremony resolves, signing results arrive, activation signatures arrive, governance unblocks, session boundaries occur |
| L2 Progress | with a tolerated coalition, enough honest bidders, and `StrongHonesty`, a started rotation leads to an epoch transition, not an abort | as L1, plus the bidder set is stable |
| L3 PostHandoverCompletion | `ActivatingKeys` leads to an epoch transition even if safe mode disables rotations meanwhile | as L1 |

The `L` properties are checked as bounded invariants on the `fair`
instance. Where Apalache's temporal mode also fits on `main`, `L1` is expected
to fail there: the retry loop behind `NC3`, and `W5` if reachable, are fair
traces that never return to `Idle`. Those are results to record, not
modelling errors.

### Witnesses (must be reachable)

| Id | Trace |
| --- | --- |
| W1 FullRotationWithHandover | a UTXO chain completes keygen, handover, activation; epoch advances |
| W2 RecoverFromKeygenFailure | a keygen fails, an offender is banned, the re-auctioned set succeeds |
| W3 AbortAtSizeFloor | bans push candidates below the floor; rotation aborts to `Idle` |
| W4 CompleteDespiteSafeMode | safe mode disables rotations during `ActivatingKeys`; the epoch still advances |
| W5 HandoverVerificationLivelock | see below |

**W5, a candidate finding from code reading.** If a handover verification
signing fails with an empty offender set (`offenders()` in the signing pallet
drops the set when it exceeds half the candidates), the chain enters
`Failed { ∅ }`. The validator, in `KeyHandoversInProgress`, sees `Failed(∅)`,
counts zero failed candidates, extends `banned` with nothing, and calls
`try_start_key_handover`. `key_handover` on the failed chain hits the
`log_or_panic!("Key handover initiated during invalid state")` branch and
leaves the status unchanged. The next block repeats. No abort edge applies and
`force_rotation` requires `Idle`. If the model reaches this trace, it becomes a
Linear issue with the trace attached. If it does not, the README records why.

## Verification

### Instances (`harness.qnt`)

| Instance | Validators | Chains | Purpose |
| --- | --- | --- | --- |
| `ceremonyStrong` | n=4, one Byzantine | (ceremony layer only) | `C1`–`C3`, `SeamSound` |
| `ceremonySplit` | as above, honest may split | (ceremony layer only) | `NC1` |
| `ceremonyOutage` | as above, honest may time out | (ceremony layer only) | `NC2`: the failure-threshold drop is unreachable at n=4/f=1 unless honest nodes miss the timeout |
| `main` | n=4, one Byzantine | one UTXO active, one non-UTXO active | all safety properties, PF1–PF4, NC3, W1, W2, W4–W7 |
| `uninit` | as `main` | adds one non-UTXO uninitialised chain | `NotInitialised` activation path, `R6` exception |
| `split` | as `main`, weak oracle | as `main` | safety under the split, `W3`, honest-banned witness |
| `fair` | as `main`, fair environment | as `main` | `L1`–`L3` as bounded invariants |

At n=4 the size floor is 3 (`min_size = 2`, and 70 percent of 4 rounds to
3), so `W3` needs two bans. Under `StrongHonesty` with one Byzantine only one
validator can ever be banned, so `W3` is expected on `split`, not `main`.

### Bounds and budget

- `ceremony.qnt`: exhaustive at depth 6.
- The handover key-mismatch branch (`Err(Default)`) is an unattributed
  failure under the oracle, so it is exercised only at the ceremony layer.
- `chain.qnt` and `validator.qnt` over the oracle: one rotation is about 20
  steps, one with a retry about 30. Simulation (`quint run`) at depth 40,
  20k samples. Exhaustive (`quint verify`) at depth 30.
- Total `./check.sh --verify` budget about 15 minutes, matching the keygen
  model. Depth compression (see Scheduling) is the first lever if a property
  does not fit; dropping to one chain is the second and must be recorded.

### Liveness checking

Two routes. Apalache's temporal mode where the instance fits. Always, the
`fair` instance: the environment is restricted to the progressing choice at
every point (every honest report arrives, every timeout fires when needed,
every signing and activation result is `Ok`, governance unblocks
immediately), which turns each `L` property into a bounded invariant with a
block counter: a rotation started at block `b` is `Idle` again by `b + K`.
`K` is computed from the longest fair trace and recorded in the README. Happy
paths also get deterministic `run` tests, checked by `quint test`, as the
keygen model does for `Done`-reachability.

### `check.sh`

Same shape as `engine/multisig/quint/check.sh`: typecheck every module, run
`quint test`, simulate every invariant and witness with `--main` routing per
instance, run the `MUST_VIOLATE` section with the exit condition inverted, and
print witness counts so a vacuous pass is visible. `--verify` adds the Apalache
passes.

### Counterexamples become Rust tests

Any violation that survives review is reproduced as an integration test in
`state-chain/cf-integration-tests/src/authorities.rs` before any fix. The
model is the search tool; the Rust test is the regression guard.

### Deferred

- Trace replay against the validator pallet mock (approach B3 in the scoping
  discussion): a Rust test consuming ITF traces from the harness. Only covers
  the validator layer. Separate ticket once the model exists.
- EVM multi-vault activation fan-out (`MultiVaultActivator`): the case where
  some vaults under one key are uninitialised and others active.
- n=7, two Byzantine.

## Known gaps

Recorded in the README from the first commit:

- One vault per chain.
- Auction economics, bids and delegation are abstract. Delegation has its own
  proptests in `cf-validator/src/delegation.rs`.
- Broadcast barriers and which key signs an in-flight transaction.
- Reputation, slashing, offence accounting, epoch expiry, history cleanup.
- No block clock: timeouts are events, not durations.
- The session pallet is two deterministic steps; its internal queueing is not
  represented.

## Build order

1. `types.qnt`: ids, tags, keys, thresholds. Typecheck.
2. `ceremony.qnt` with `C1`–`C3`, `SeamSound`, `NC1`, `NC2`. Verify at depth
   6 before moving on; this layer is the contract everything else assumes.
3. `chain.qnt` with `H1`–`H4`, `PF1`–`PF4`, on a single chain first, then
   the cons combinator over two.
4. `validator.qnt` with `R1`–`R6`, `NC3`, driven by the chain layer.
5. `harness.qnt`: `main`, `uninit`, `split`, `fair` instances; `W1`–`W5`;
   deterministic run tests for the happy paths.
6. `check.sh` and `README.md` with the status table, witness counts, gotchas,
   and known gaps.
7. Linear issues for any surviving violation, each with its trace and a
   matching integration test.

## Risks

- **State-space blow-up.** Mitigated by the oracle boundary, depth
  compression, and instance shrinking, in that order. Each retreat is
  recorded.
- **Model and implementation drift.** Every transition in `chain.qnt` and
  `validator.qnt` cites the Rust function it mirrors. Trace replay (Deferred)
  is the structural fix.
- **Fairness hiding real livelocks.** `NC3` and `W5` run in the unfair
  `main` instance, never only in `fair`.
- **Temporal mode not fitting.** The `fair` instance is the fallback and
  always runs.
- **Vacuous properties.** Every invariant has a witness that exercises its
  antecedent, and witness counts are printed by `check.sh`.
