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
./check.sh            # typecheck, tests, simulation, negative controls (~3 min)
./check.sh --verify   # add exhaustive Apalache checks (~18 min in total)
```

The two parameterised modules — `ceremonyCheck` in `ceremony.qnt` and
`rotation` in `validator.qnt` — declare `const`s and cannot run directly, so
every check on those layers targets an instance in `harness.qnt` via `--main`.
The unparameterised modules (`types`, `chain`, and `ceremony` itself) run on
their own, which is how `check.sh` invokes their unit tests.

```bash
quint run harness.qnt --main=main --invariant=PF_NoPanics --max-steps=40
quint test harness.qnt --main=main
```

## Source-file correspondence

```text
ceremony.qnt  <-> state-chain/pallets/cf-threshold-signature/src/response_status.rs, lib.rs (progress_rotation)
chain.qnt     <-> state-chain/pallets/cf-threshold-signature/src/key_rotator.rs, lib.rs (on_key_verification_result, terminate_rotation),
                  state-chain/pallets/cf-vaults/src/vault_activator.rs, state-chain/runtime/src/chainflip/cons_key_rotator.rs
validator.qnt <-> state-chain/pallets/cf-validator/src/lib.rs (on_initialize, rotation helpers, session hooks), helpers.rs
```

## Status

Numbers below come from one `./check.sh` and one `./check.sh --verify` run on
2026-09-08 (quint 0.32.0, Apalache 0.56.1, darwin 24.6.0, 48 GiB RAM), except
where a row is marked as carried over from an earlier task report. Simulation
draws a fresh seed on each run, so witness counts move by a few percent between
runs; the verdicts do not. Verification times move too, and by more: the three
`main1` entries took 166 s / 129 s / 422 s here against the 140 s / 248 s / 189 s
measured in the Task 7b tractability study below. Only the verdicts are stable.
The whole `--verify` pass took 17 m 33 s wall clock, of which about 15 minutes is
the Apalache section.

### Ceremony layer (`ceremony.qnt`)

Instances: `ceremonyStrong` (n=4, f=1, `STRONG`), `ceremonySplit`,
`ceremonyOutage`.

| Property | Meaning | Simulated | Verified |
|---|---|---|---|
| `C1_AcceptanceUnanimity` | a key is accepted only when every candidate voted for it | `ceremonyStrong`, depth 6, 20000 samples, `[ok]` | `ceremonyStrong`, depth 6, `[ok]` in 9.1 s |
| `C2_OffendersAreParticipants` | reported offenders are always drawn from the candidate set | `ceremonyStrong`, depth 6, 20000 samples, `[ok]` | `ceremonyStrong`, depth 6, `[ok]` in 6.7 s |
| `C3_HonestNeverOffender` | under strong honesty an honest node is never attributed | `ceremonyStrong`, depth 6, 20000 samples, `[ok]` | `ceremonyStrong`, depth 6, `[ok]` in 6.8 s |
| `SeamSound` | the ceremony result the chain layer consumes matches what the ceremony resolved | `ceremonyStrong`, depth 6, 20000 samples, `[ok]` | `ceremonyStrong`, depth 6, `[ok]` in 7.0 s |

Verification of this layer needs `--apalache-config=apalache-no-deadlocks.json`:
the ceremony is a one-shot model, so once `result` is set every action in `step`
is disabled and Apalache reports that intended terminal state as a deadlock.
`--apalache-config` takes a **file path**; the inline-JSON form is not accepted.

### Chain and validator layers (`chain.qnt`, `validator.qnt`)

Instances: `main` (two chains, one UTXO, `STRONG`, unfair), `uninit` (adds an
uninitialised chain), `split` (`STRONG = false`), `fair` (fair scheduler,
`K_BLOCKS = 14`), `main1` (one UTXO chain; exists only so that something on this
layer verifies above depth 1).

| Property | Meaning | Simulated (instance, depth, samples) | Verified |
|---|---|---|---|
| `R1_BannedNeverAuthority` | a banned node is never a primary candidate, never queued, never an authority in `Idle` | `main`, `split`; depth 80, 20000 | no |
| `R2_SizeFloor` | the queued set respects `max(min_size, contraction floor)` | `main`, `split`; depth 80, 20000 | `main1`, depth 2, `[ok]` in 166 s |
| `R3_SharingSetValidity` | sharing participants are unbanned current authorities, at least the success threshold | `main`, `split`; depth 80, 20000 | no |
| `R4_TransitionGating` | the epoch advances only when every chain is `Complete`, with the final candidate set queued | `main`, `uninit`, `split`; depth 80, 20000 | `main1`, depth 2, `[ok]` in 129 s |
| `R5_NoNextKeyAfterAbort` | in `Idle` no active chain holds a key for a future epoch | `main`, `uninit`, `split`; depth 80, 20000 | no |
| `R6_KeyEpochAgreement` | active chains agree on the current key epoch | `main`, `uninit`, `split`; depth 80, 20000 | no |
| `R7_NoAbortAfterActivation` | DESIGN.md's `L3 PostHandoverCompletion`, safety half: no abort edge exists past `ActivatingKeys` (the liveness half is the `W4` witness) | `main`, `split`, `fair`; depth 80, 20000 (2000 on `fair`) | no |
| `H2_UtxoAlwaysHandsOver` | a UTXO chain with a key never completes handover without running the ceremony | `main`, `split`; depth 80, 20000 | no |
| `H3_NextKeyOnlyAfterActivation` | the next-epoch key exists only during activation and the session steps | `main`, `uninit`, `split`; depth 80, 20000 | `main1`, depth 2, `[ok]` in 422 s |
| `H4_ConsSoundness` | a merged `Failed` comes from a failed chain and carries exactly its offenders | `main`, depth 80, 20000 | no — and *not* verifiable on `main1`, where it is trivially true on one chain |
| `NoUnexpectedLogErrors` | no `log::error!` site that `on_initialize`/`activate_keys` treat as impossible is reached | `main`, `uninit`; depth 80, 20000 | no |
| `PF_NoPanics` | DESIGN.md's `PF1`–`PF4` collapsed into one: no assertion site the model tracks is reachable. `PF3` is not among them — it holds by construction (`startHandover` only creates `AwaitingKeyHandover` when `activeKey` is `Some`) and its Rust site is the `.expect` in `cf-threshold-signature/src/lib.rs` (handover progress), not `key_rotator.rs` | **`[violation]` on `main`** (depth 80, 20000; reported, not enforced); `[ok]` on `fair`, depth 80, 2000 | no |
| `L1_Termination` | a started rotation finishes within `K_BLOCKS` | `fair`, depth 80, 2000 | `fair`, depth 1, `[ok]` in 78 s |
| `L2_Progress` | no rotation aborts under a fair scheduler | `fair`, depth 80, 2000 | `fair`, depth 1, `[ok]` in 73 s |

`PF_NoPanics` on `main` is a recorded finding, not a regression: the
handover-verification livelock (`W5`) reaches the `handover_invalid_state`
assertion. `check.sh` prints its verdict but does not gate on it. See
"Findings" (F1) for the classification and the Rust path.

### Witness coverage

Counts from the `./check.sh` run above. `W5`, `W6` and `W10` are the only
witnesses that are printed but not required-positive; every other witness listed
here fails the script if it reads 0. The `W10` figure comes from the Task 9
triage run rather than the `./check.sh` run the rest of the table is from; it
reads 8069 / 20000 (40.34%) on `split`.

`W2` and `W4` are conjunctions over history counters (`epochsAdvanced` with
`keygenRestarts`, and `epochsAdvanced` with `safeModeOffWhileActivating`), so a
trace can satisfy them across *two* rotations — the failure or the safe-mode
window in the first, the completed epoch in the second — rather than in one.
They are kept that way deliberately: they are reachability witnesses, and
tightening them to a single rotation would make them harder to fire without
adding coverage. The single-rotation reading of `W4` is pinned separately and
deterministically by `w4CompleteDespiteSafeModeTest` in `harness.qnt`.

| Witness | Instance | Count / samples | Required |
|---|---|---|---|
| `W1_FullRotationWithHandover` | `main` | 240 / 20000 (1.20%) | yes |
| `W2_RecoverFromKeygenFailure` | `main` | 130 / 20000 (0.65%) | yes |
| `W3_AbortAtSizeFloor` | `main` | 17704 / 20000 (88.52%) | no (required on `split`) |
| `W4_CompleteDespiteSafeMode` | `main` | 172 / 20000 (0.86%) | yes |
| `W5_HandoverVerificationLivelock` | `main` | 112 / 20000 (0.56%) | no — a finding, not coverage |
| `W6_AbortSharingUnavailable` | `main` | **0 / 20000 (0.00%)** | no — reads 0 on this instance, see "Known gaps" |
| `W7_HandoverRetry` | `main` | 543 / 20000 (2.71%) | yes |
| `W10_ForcedRotationWhileBroadcastsPending` | `main` | 6739 / 20000 (33.70%) | no — a finding, not coverage |
| `W9_UninitialisedChainCompletesWithoutKey` | `uninit` | 32 / 20000 (0.16%) | yes |
| `W3_AbortAtSizeFloor` | `split` | 16734 / 20000 (83.67%) | yes |
| `W8_HonestBannedUnderSplit` | `split` | 981 / 20000 (4.91%) | yes |
| `W1_FullRotationWithHandover` | `fair` | 2000 / 2000 (100.00%) | yes |
| `W2_RecoverFromKeygenFailure` | `fair` | 2000 / 2000 (100.00%) | yes |
| `wResolvedSuccess` | `ceremonyStrong` | 1310 / 20000 (6.55%) | yes |
| `wResolvedFailure` | `ceremonyStrong` | 18690 / 20000 (93.45%) | yes |
| `wByzantinePunished` | `ceremonyStrong` | 16048 / 20000 (80.24%) | yes |

### Negative controls

Each must report `[violation]`; an `[ok]` fails `check.sh` loudly, because it
means the model has lost the power to see that bug class.

| Control | Instance | Observed |
|---|---|---|
| `NC1_HonestNeverOffender_MustFailHere` | `ceremonySplit` | `[violation]` — without strong honesty an honest node ends up in the resolved offender set |
| `NC2_DropNeverEmpties_MustFailHere` | `ceremonyOutage` | `[violation]` — the failure-threshold drop empties a non-empty offender set to `Set()` |
| `NC3_EveryRetryBans_MustFailHere` | `main` | `[violation]` — a retry with empty offenders reissues the ceremony without banning anyone |

### Tractability

Copied from the Task 7b investigation. `fun-arrays` means
`--apalache-config` pointing at a file containing
`{"checker": {"smt-encoding": {"type": "fun-arrays"}}}`. Heaps differ across
sources: the Task 6 rows ran at 24 GiB, the Task 7 rows at Apalache's 4 GiB
default, the Task 7b rows at 8 GiB (one at 24 GiB, marked). Runs made in
Task 7b were capped at 600 s (one at 570 s, marked).

| lever | source | instance | property | depth | flags | verdict | seconds | notes |
|---|---|---|---|---|---|---|---|---|
| — | Task 6 | main | R2_SizeFloor | 1 | `-Xmx24576m` (24 GiB), default encoding | [ok] | 72 | |
| — | Task 6 | main | R2_SizeFloor | 2 | 24 GiB, default encoding | no verdict | 8138 (killed, 2 h 16 m) | |
| — | Task 6 | main | R2_SizeFloor | 8 | 4 GiB (Apalache default) | Java heap space | 181 | evidence that the default heap OOMs |
| — | Task 7 | fair | L1_Termination | 1 | 4 GiB default | [ok] | 76 | |
| — | Task 7 | fair | L1_Termination | 2 | 4 GiB default | Java heap space | 181 | 181 s here is a coincidence, not the depth-8 figure above |
| C tuning | Task 7b | main | R2_SizeFloor | 2 | 8 GiB, default encoding | timeout | 570 (cap) | heap alone is not the constraint |
| C tuning | Task 7b | main | R2_SizeFloor | 2 | 8 GiB, inline-JSON config | tool error | 6 | one row for two attempts (deviation M1) |
| C tuning | Task 7b | main | R2_SizeFloor | 1 | 8 GiB, fun-arrays | [ok] | 40 | vs 72 s at 24 GiB, default encoding — not heap-controlled |
| C tuning | Task 7b | main | R2_SizeFloor | 2 | 8 GiB, fun-arrays | timeout | 600 (cap) | best lever-C configuration, still no verdict |
| B one chain | Task 7b | main1 | R2_SizeFloor | 2 | 8 GiB, fun-arrays | **[ok]** | 140 | first depth-2 verdict on the validator layer |
| B one chain | Task 7b | main1 | R4_TransitionGating | 2 | 8 GiB, fun-arrays | **[ok]** | 248 | |
| B one chain | Task 7b | main1 | H3_NextKeyOnlyAfterActivation | 2 | 8 GiB, fun-arrays | **[ok]** | 189 | |
| B one chain | Task 7b | main1 | R2_SizeFloor | 3 | 8 GiB, fun-arrays | timeout | 600 (cap) | extra probe (deviation M2) |
| B one chain | Task 7b | main1 | R2_SizeFloor | 4 | 8 GiB, fun-arrays | timeout | 600 (cap) | |
| A per-phase block | Task 7b | main | R2_SizeFloor | 2 | 8 GiB, fun-arrays | Java heap space | 219 | OOM where the unsplit model merely ran long |
| A per-phase block | Task 7b | main | R2_SizeFloor | 2 | 24 GiB, fun-arrays | timeout | 600 (cap) | no verdict even with 24 GiB |
| A+B | Task 7b | main1 | R2_SizeFloor | 2 | 8 GiB, fun-arrays | Java heap space | 165 | controlled comparison: [ok] in 140 s *without* the split, OOM *with* it |
| D symbolic sim | Task 7b | main | all 11 safety invariants, conjoined | 25 | 8 GiB, fun-arrays, `--random-transitions` | timeout | 600 (cap) | one run, not one per invariant (deviation M3) |
| D symbolic sim | Task 7b | main | R2_SizeFloor | 25 | 8 GiB, fun-arrays, `--random-transitions` | timeout | 600 (cap) | one invariant alone is no better |

**The validator layer's exhaustive coverage — depth 1 on `main`/`fair` and
depth 2 on `main1` — does not reach a completed rotation** (a rotation needs
roughly twenty environment deliveries plus the blocks that consume them), so the
evidence for that layer is the randomised simulation at depth 80, where about
1.2% of `main` traces complete a rotation and every `fair` trace does. Read the
`[ok]`s in the "Verified" column above as a two-step neighbourhood of the initial
state, not as a proof over rotations. Only the ceremony layer is exhaustively
covered end to end, at depth 6, which is the full length of a ceremony.

### Not verified / deferred

- **Temporal mode.** Not attempted. The validator layer does not fit
  exhaustively at depth 2 on two chains, so temporal properties over it are out
  of reach; `L1_Termination` and `L2_Progress` on the `fair` instance are the
  bounded stand-in.
- **n = 7.** Every instance is n = 4, f = 1. At n = 4 the success threshold is
  3 and only one validator can ever be banned under `STRONG`, so three unbanned
  current authorities always remain and `W6_AbortSharingUnavailable` reads 0 on
  this instance. Whether the edge is reachable at all is not settled here.
- **Multiple vaults per chain.** One vault per chain throughout.
- **Per-phase block transitions.** Tried in full in Task 7b (splitting `block`
  into per-phase arms), measured strictly worse — on `main1` at depth 2 it turns
  a 140 s `[ok]` into an out-of-memory failure at 165 s — and reverted.

## Gotchas

- Never write `setOfMaps(D, C).oneOf()`; Apalache rejects it, `quint run` does not.
- Draw `nondet` values from `powerset()` and decode them; never constrain a
  free draw inside `all { }`.
- `Option` is not in the standard library; `types.qnt` defines it.
- Without `--main`, quint picks the module named after the file, which is
  wrong for every multi-module file here.
- `fail` is a built-in; do not name an action `fail`.
- `getOnlyElement` crashes the Quint-to-Apalache IR converter; `ceremony.qnt`
  folds over the singleton set instead. `chooseSome` is not an alternative —
  the simulator does not implement it.
- `--apalache-config` takes a file path. The inline-JSON form is not accepted
  and the tuning is silently never applied.
- `quint verify` leaves an Apalache server on port 8822, so verify runs must be
  sequential; a second one launched against a busy server just blocks on the
  port and its wall-clock time is meaningless.
- Interrupting `quint run` orphans a `~/.quint/rust-evaluator-*/quint_evaluator`
  child that spins at ~285% CPU and wrecks every subsequent measurement. Follow
  any interrupt with `pkill -f quint_evaluator` (and `pkill -f apalache` after
  an interrupted `--verify`).
- macOS ships no `timeout`; `check.sh` falls back to
  `perl -e 'alarm shift; exec @ARGV'`.

## Known gaps

From `DESIGN.md`:

- One vault per chain.
- Auction economics, bids and delegation are abstract. Delegation has its own
  proptests in `cf-validator/src/delegation.rs`.
- Broadcast barriers and which key signs an in-flight transaction.
- Reputation, slashing, offence accounting, epoch expiry, history cleanup.
- No block clock: timeouts are events, not durations.
- The session pallet is two deterministic steps; its internal queueing is not
  represented.

Discovered while building and checking the model:

- UTXO chains are assumed to hold a genesis key. The bootstrap-without-key path
  is exercised only by `utxoWithoutKeySkipsHandoverTest` in `chain.qnt`, not by
  any instance simulation.
- `W6_AbortSharingUnavailable` reads 0 on every instance here: the
  `SharingUnavailable` abort needs the current authority set to fall below the
  success threshold, which n = 4 with at most one ban cannot produce. The abort
  edge therefore has no coverage. See "Findings", "Coverage gaps observed".
- `W3_AbortAtSizeFloor` fires in ~85% of traces, but for the ordinary
  not-enough-qualified-bidders reason rather than the contraction floor it is
  named for. It is a weak signal: it does not discriminate the two paths.
- `H4_ConsSoundness` has no exhaustive coverage at all. It is trivially true on
  the one-chain `main1` instance — the only one that verifies above depth 1 —
  so `main1` must never be cited as evidence for it.
- Honest signing liveness is not assumed. `StrongHonesty` constrains ceremony
  *attribution* only, so `verificationResult` may return `isOk = false` with an
  empty offender set no matter how many participants are honest. F1's
  precondition — more than half the receiving participants failing one signing
  ceremony — is therefore free in the model, while in production it needs a
  real mass outage. Any liveness read off F1's trace frequency is an artefact of
  this gap.
- The `fair` instance runs at ~21-33 traces/s against ~2000-6000/s elsewhere,
  because no trace aborts early; `check.sh` runs it at 2000 samples for that
  reason.

## Findings

Triage of every candidate the checking tasks raised: the `PF_NoPanics`
violation on `main`, the `NC3` control, the `H4_ConsSoundness` non-observation,
the zero `W6` count, and `force_rotation`'s missing broadcast gate. Each
surviving item has a deterministic `run` in `harness.qnt` that drives the
behaviour with concrete arguments — those are the model-side regression guards,
and `quint test harness.qnt --main=main` (in `check.sh`) runs them.

Nothing in this section changes any Rust. Where a finding is Real, the next
step is a `cf-integration-tests/src/authorities.rs` regression test, tracked
separately.

### F1 — Handover-verification failure livelocks the rotation

**`PF_NoPanics` / `W5_HandoverVerificationLivelock`. Classification: Real. Repro test: `w5LivelockReproTest`. Linear: [PRO-3126](https://linear.app/chainflip/issue/PRO-3126).**

When the *verification signing* that follows a successful key-handover ceremony
fails, `on_key_verification_result` routes the error to `terminate_rotation`,
which parks that chain in `KeyRotationStatus::Failed { offenders }` — not in
`KeyHandoverFailed`, the variant `key_handover` knows how to resume from. The
offender set can be empty: `CeremonyContext::<T, I>::offenders()` returns an
empty `Vec` whenever the set it would report exceeds half the candidates, which
is exactly the mass-unresponsiveness case. **That empty set is the whole
precondition, and it is a strong one:** the handover-verification signers are
exactly `receiving_participants`, i.e. the new authority candidates
(`cf-threshold-signature/src/lib.rs:874-879`), so any *non-empty* offender set
necessarily intersects the candidates and `on_initialize` takes the
restart-keygen branch instead; reaching the livelock therefore needs strictly
more than half of the new authority set non-responsive or heavily blamed within
that single signing ceremony. The model reaches it under `STRONG` only because
signing liveness is not part of `StrongHonesty` — `validator.qnt`'s
`verificationResult` admits `isOk = false` with an empty offender set freely
(see "Known gaps"). `ConsKeyRotator::status()` then folds
that chain's `Ready(Failed(∅))` with the other chains' `Ready(KeyHandoverComplete)`
into `Ready(Failed(∅))`. Back in `on_initialize`'s `KeyHandoversInProgress` arm,
`offenders.intersection(candidates).count()` is 0, so control takes the
"Retrying with a new participant set" branch, extends `rotation_state.banned`
with nothing, and calls `try_start_key_handover`, which calls
`KeyRotator::key_handover` on every chain. On the `Failed` chain that reaches
`log_or_panic!("Key handover initiated during invalid state")` and leaves the
storage untouched; on the chains already at `KeyHandoverComplete` it logs "Key
handover already complete" and does nothing. The phase stays
`KeyHandoversInProgress`, no state changes, and the next block repeats the whole
sequence. No abort edge applies (`try_start_key_handover` aborts only on safe
mode or an unavailable sharing set) and `force_rotation` requires `Idle`, so the
chain cannot leave the rotation. `log_or_panic!` panics in debug and test builds
but only logs in release, so the production consequence is a **livelock — the
rotation never completes and never aborts — not a chain halt**. The one
operational escape found in both the model and the Rust is governance turning
`SafeMode::authority_rotation_enabled` off: the next block's
`try_start_key_handover` then aborts the rotation before it touches
`key_handover`. Rust path: `cf-threshold-signature/src/lib.rs`
(`offenders()`, `on_key_verification_result` → `terminate_rotation`) →
`cf-threshold-signature/src/key_rotator.rs` (`status`, `key_handover`'s
`other => log_or_panic!` arm) → `runtime/src/chainflip/cons_key_rotator.rs`
(`status`) → `cf-validator/src/lib.rs` (`on_initialize`'s
`KeyHandoversInProgress` arm, `try_start_key_handover`). Simulation reaches it
in 0.56–0.60% of `main` traces at depth 80; seeds `0xed970f786a6883bf`,
`0x46a667fb9d3075b`, `0x4fa4999ea5463ae4`, `0x87876582b1418c09` all reproduce
the `PF_NoPanics` violation at `--max-steps=80`.

### F2 — A retry that bans nobody re-runs the same ceremony

**`NC3_EveryRetryBans_MustFailHere`. Classification: By design. Repro test: `nc3RetryWithoutBanReproTest`. Linear: none (by design).**

`try_restart_keygen` calls `rotation_state.ban(offenders)` and re-resolves the
auction excluding the banned set. With an empty offender set — again the
`offenders()` liveness-threshold drop, or a chain that failed with nobody
attributable — the banned set does not grow, the auction returns the same
winners, and the identical keygen is reissued. The control asserts that this
never happens precisely so that it *must* fail: an `[ok]` would mean the model
had lost the ability to see unattributed-retry loops. This is the intended
behaviour of the Rust — the alternative, aborting whenever a failure cannot be
attributed, hands any single unattributable failure the power to cancel a
rotation — and the retry is bounded in practice by the epoch continuing. The
control stays in `check.sh`'s `MUST_VIOLATE` list. Note that F1 is the case
where this same unattributed retry does *not* make progress; F2 on its own does.

### F3 — `force_rotation` bypasses the pending-broadcast gate

**`W10_ForcedRotationWhileBroadcastsPending`. Classification: By design; the divergence is undocumented and its consequence is outside this model. Repro test: `w10ForcedRotationBypassesBroadcastGateTest`. Linear: [PRO-3127](https://linear.app/chainflip/issue/PRO-3127).**

`on_initialize`'s `Idle` arm will not start a rotation while
`T::RotationBroadcastsPending::rotation_broadcasts_pending()` holds: it emits
`PreviousRotationStillPending` and waits. The `force_rotation` extrinsic checks
only `CurrentRotationPhase == Idle` and `SafeMode::authority_rotation_enabled`,
then calls the same `start_authority_rotation`. Governance can therefore start a
rotation in exactly the state the hook declines to act on. The model reproduces
this faithfully — `forceRotation` is guarded by `Idle` and `rotationEnabled`
only — and `W10` reads 6739 / 20000 (33.70%) on `main` and 8069 / 20000 (40.34%)
on `split` at depth 80. **Read those percentages as reachability, not as
frequency:** they measure how often the model's unconstrained
`toggleBroadcastsPending` adversary action happens to be set when
`forceRotation` fires, and are not an estimate of anything in production. They
are also a lower bound on the bypass, because the flag is set only when the
forced rotation gets past the auction — a forced rotation that aborts
immediately is not counted. Every safety invariant still holds in those traces,
but that is weak evidence: broadcast barriers and which key signs an in-flight
transaction are explicitly out of scope (see "Known gaps"), so the model cannot
see the consequence the hook's gate exists to prevent. Classified By design
because `force_rotation` is a governance override and overriding is what it is
for; the ticket is to record that intent in the code (the gate's absence reads
as an oversight next to the hook) and to confirm the pending-broadcast case is
one governance is willing to accept. Rust path: `cf-validator/src/lib.rs`,
`force_rotation` against the `RotationPhase::Idle` arm of `on_initialize`.

### Coverage gaps observed

Neither of these is a finding; both are limits of the instances, recorded so
that a future reader does not mistake a zero or an absence for evidence.

- **`W6_AbortSharingUnavailable` reads 0 on `main` and on `split`.** The
  `SharingUnavailable` abort fires when `select_sharing_participants` cannot
  assemble the success threshold from the unbanned current authorities. Every
  instance here is n = 4, so the threshold is 3, and under `STRONG` at most one
  validator is ever banned — three unbanned current authorities always remain.
  On `split` honest nodes can be banned too, but the same arithmetic still
  leaves the abort out of reach at 20000 samples. The edge has **no coverage**;
  reaching it needs n = 7, which is the "Not verified / deferred" item above.
- **`H4_ConsSoundness` held in every sampled trace at depth 80 on `main`, and a
  mixed-`Ready` empty failure was never observed outside the F1 path.** In the
  F1 traces the `Failed(∅)` the combinator produces *does* come from a genuinely
  failed chain carrying no offenders, so `H4` is satisfied there. The combinator
  can in principle synthesise a `Failed(∅)` from two non-failed `Ready`
  statuses — `consPairTest` in `chain.qnt` pins that behaviour, and the Rust's
  own `all_ready_but_diff_statuses_is_failure` test does too — which would
  violate `H4`. That state was **not observed**; it is **not** thereby
  unreachable. `H4` has no exhaustive coverage either: it is trivially true on
  the one-chain `main1` instance, the only one that verifies above depth 1, so
  `main1` can never be cited for it. The property's status is "not falsified by
  simulation", nothing stronger.
