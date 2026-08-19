# Quint model of the multisig keygen ceremony

**Date:** 2026-08-19
**Status:** implemented
**Scope:** `engine/multisig/src/client/{common,keygen}/`

See `engine/multisig/quint/README.md` for current results: what is exhaustively
verified vs. simulation-only, witness coverage, and the two permanent negative
controls.

## Motivation

`frost-security-review.md` (2026-05-15) assessed the FROST implementation against
published findings. Four of five items were already mitigated; one — the
Trail of Bits per-ceremony coefficient-length check — had regressed in PR #3248
and has since been fixed (`keygen_detail.rs`, `validate_commitments`).

The review's closing recommendation was not to fix another bug but to change how
the subtle part gets audited:

> For #2 a future auditor can be given a strong, specific brief: [...] please
> pressure-test the *attribution* logic in `broadcast_verification.rs` (the
> `threshold_for_broadcast_verification` / `find_frequent_element` quorum math)
> rather than the existence of the check.

That is a model-checking problem. Attribution correctness is a property over all
Byzantine behaviours, and the existing Rust tests each exercise one
hand-written scenario. This spec describes a Quint model that checks the
property class exhaustively at small party counts.

The goal is Byzantine **agreement and attribution**, not cryptographic
soundness. Elliptic-curve operations, ZKPs, and hash commitments are assumed
sound and modelled abstractly.

## What the model must cover

Keygen, all ten stages (`KeygenStageName`, `client/common.rs`):

```text
PubkeyShares0 → HashCommitments1 → VerifyHashCommitmentsBroadcast2
              → CoefficientCommitments3 → VerifyCommitmentsBroadcast4
              → SecretSharesStage5 → ComplaintsStage6 → VerifyComplaintsBroadcastStage7
              → BlameResponsesStage8 → VerifyBlameResponsesBroadcastStage9
```

Key handover (resharing) is in scope from the start: `sharing_participants` and
`receiving_participants` are distinct, possibly overlapping sets with an index
remapping (`future_index_mapping`). Plain keygen is the instance where both sets
are all parties.

Signing and the `CeremonyRunner` timeout/delay layer are out of scope for this
model. Signing reuses the same echo-broadcast primitive, so the lemma layer
below applies to it unchanged if it is modelled later.

## Two broadcast primitives, not one

The implementation contains two distinct agreement mechanisms. Collapsing them
into one model would grant `PubkeyShares0` attribution power it does not have,
masking downstream bugs.

**Echo-broadcast** (`BroadcastStage` + `verify_broadcasts`) is the two-round
primitive used by every `Verify*` stage. Each party re-broadcasts the full set
it received, then `find_frequent_element` resolves each sender's value against a
2/3 quorum. It has three outcomes, and the distinction between the last two is
the security-relevant one (`broadcast_verification.rs`):

- a quorum agrees on a value → that value is used;
- a quorum agrees the party sent *nothing* → attributable, reported;
- no quorum forms either way → the stage fails, **nobody** is reported.

Claims of non-receipt are unfalsifiable, so a colluding minority large enough to
deny quorum could otherwise manufacture agreement that honest parties failed to
broadcast — and blame is what gets nodes banned from the retried ceremony.

**Single-round quorum vote** (`PubkeySharesStage0`) takes a quorum over
`sharing_participants.len()` only, with no echo round. It can never attribute:
all three of its failure paths report an empty set, because a party without the
original key cannot tell which pubkey shares are correct.

### Quorum arithmetic

`threshold_from_share_count(n) = (2n − 1) / 3` (integer division), and
`find_frequent_element` requires `count > threshold`. A quorum is therefore
`⌊(2n − 1)/3⌋ + 1`. Byzantine tolerance follows `n ≥ 3f + 1`, giving n=4/f=1 and
n=7/f=2 as the natural check configurations.

## Architecture

Five modules under `engine/multisig/quint/`:

| Module | Contents |
| --- | --- |
| `types.qnt` | party indices, roles, abstract `Value`/`Share` types, ceremony parameters |
| `broadcast.qnt` | lemma layer: both broadcast primitives, modelled concretely |
| `oracle.qnt` | the idealised contract `broadcast.qnt` discharges; consumed by `keygen.qnt` |
| `keygen.qnt` | the ten-stage machine over the oracle, including complaints and blame |
| `harness.qnt` | instantiations, invariants, and runs |

The split mirrors the Rust, where `BroadcastStage` is generic over
`BroadcastStageProcessor` and each stage supplies only `init` and `process`.

### The abstraction boundary

`oracle.qnt` is the load-bearing part of the design and is stated as an explicit
contract rather than left implicit:

```text
EchoBroadcast(senders, values) returns one of:
  Agreed(m)        — m : idx -> Value, identical at every honest party,
                     and m[i] = values[i] for every honest sender i
  Attributed(bad)  — bad ⊆ parties, bad ∩ honest = ∅, bad ≠ {}
  Unattributed     — stage fails, nobody blamed

QuorumVote(sharing_participants, values) returns one of:
  Agreed(v)        — identical at every honest party
  Failed           — never attributes
```

`broadcast.qnt` models both primitives concretely and proves their
postconditions (L1–L6 below). `keygen.qnt` then calls the oracle as a single
atomic step, which is what makes a ten-stage model checkable at all.

This is an assume/guarantee argument discharged by hand. Its risk is a bug
living exactly at the seam. Writing the contract down means such a bug can be
attributed to a specific violated clause rather than to an unexamined
assumption, and phase 2 below adds a cross-check against the concrete module.

### Faithful vs. abstract in `keygen.qnt`

| Faithful | Abstract |
| --- | --- |
| Stage sequence 0→9 and every early-exit branch | EC points, scalars, polynomials |
| Private share delivery (per-recipient) | `share_valid(from, to)` — adversary-set for Byzantine senders |
| Coefficient-length check (`== threshold + 1`) | ZKP and hash-commitment verification (assumed sound) |
| Complaint sets, `is_blame_response_complete`, stage-9 re-verification | Share arithmetic |
| `sharing_participants` / `receiving_participants` / `future_index_mapping` | Key material |
| Membership of `reported_parties` | Aggregate pubkey value |

The coefficient-length check stays faithful rather than assumed: it is an
integer comparison, it is the check the review flagged, and under handover the
expected count derives from the *new* key's threshold — an asymmetry worth
exercising.

## Adversary model

A fixed set `byzantine ⊆ parties`, `|byzantine| = f`, chosen at initialisation.
Byzantine parties are adaptive; their choices may depend on everything they have
observed. Per stage they may:

- equivocate — send a different value to each recipient;
- withhold — send nothing to some or all recipients;
- lie in an echo round about what they received;
- corrupt private shares per-recipient (stage 5);
- complain falsely about honest parties (stage 6);
- send incomplete or invalid blame responses (stage 8);
- commit to a coefficient count other than `threshold + 1` (stage 3).

They may not open a hash commitment two ways, forge a ZKP, or read shares sent
between honest parties. These are the inherited cryptographic assumptions; the
model tests the protocol around them, not them.

### Timeouts

Timeouts are modelled as non-deterministic message absence at finalize time, not
as a real-time domain. `BroadcastStage::finalize` inserts `None` for anything
not received, so the attribution logic only ever observes present-or-absent.
Modelling absence directly captures exactly what the code sees and keeps a time
domain out of the state space. The `CeremonyRunner` timer itself is out of
scope.

## Properties

### Lemma layer (`broadcast.qnt`)

| | Property |
| --- | --- |
| L1 | **NoFalseBlame** — `reported ∩ honest = ∅` at every honest party |
| L2 | **ValueAgreement** — two honest parties returning `Agreed` return equal maps |
| L3 | **HonestValuePreservation** — an agreed map holds each honest sender's actual value |
| L4 | **SafeDivergence** — honest parties may diverge between `Agreed` and a failure outcome, but never between two *different* `Agreed` maps, and a diverging party never blames an honest party |
| L5 | **Liveness** — if every party's message is delivered and Byzantine parties behave honestly, all honest parties return `Agreed` |
| L6 | **VoteAgreement** — two honest parties returning `Agreed` from `QuorumVote` return the same value, and `QuorumVote` never reports anyone |

### Ceremony layer (`keygen.qnt`)

| | Property |
| --- | --- |
| K1 | **NoHonestBlamed** — no honest party in any honest party's final reported set |
| K2 | **NoConflictingOutcome** — no two honest parties finish `Done` with different keys or participant sets |
| K3 | **AttributionProgress** — a non-empty reported set contains at least one Byzantine party |
| K4 | **HandoverNoFalseBlame** — K1 with `sharing ≠ receiving` |
| K5 | **Termination** — every run reaches Done or Error |
| K6 | **KeyConsistency** — on Done, all honest parties derived the same key and participant set |
| K7 | **QuorumCoupling** — a stage cannot succeed for anyone unless a quorum is still participating (modelled as a constraint, with divergence covered by a witness rather than an invariant) |

L1 and K1 are the headline properties. K3 complements K1: blame must be not only
safe but productive, or an adversary can force repeated unattributed retries.

**L4-as-originally-stated is false. This was settled during planning, not left
open.** A prototype of the lemma layer produced a concrete counterexample at
n=4/f=1 in milliseconds:

> Byzantine party 4 equivocates in round 1, sending value `0` to parties 1, 2, 4
> and value `1` to party 3. Honest echoes about party 4 therefore split 2–1. In
> round 2 party 4 uses its own claim as a tie-breaker: it tells parties 3 and 4
> it sent `0` (giving `0` three votes, clearing the quorum of 3 → `Agreed`), and
> tells parties 1 and 2 it sent `1` (2 votes each way → no quorum →
> `Unattributed`). Honest party 3 proceeds to the next stage while honest
> parties 1 and 2 abort.

This is a liveness degradation, not a safety violation: no honest party is
blamed and no two honest parties commit to different values. The ceremony fails
and is retried. L4 is therefore restated as **SafeDivergence** above, and K2 is
weakened to match — full outcome agreement is not a property this protocol has,
and a model asserting it would fail immediately for a benign reason.

The finding does raise a genuine question at the ceremony layer, and prototyping
answered it. K7 was originally stated as "the proceeding group cannot be walked
all the way to a finalised key". **That is false, and it is false of the real
protocol, not just the model.** Two things came out of testing it:

1. A real faithfulness gap in the abstraction, now fixed: the model let a party
   draw a successful stage outcome regardless of how many other parties had
   already aborted. Every stage collects from all participants, so once too many
   have aborted the rest time out. This is now a modelled constraint — a stage
   succeeds for nobody unless a quorum is still participating.
2. Even with that constraint, a minority of honest parties can abort while the
   quorum completes, and at the final verify stage a single honest party can
   finalise locally while others fail. That is correct threshold behaviour, not
   a safety violation. Whether a locally-finalised key is ever *used* is decided
   outside this model, by the State Chain requiring a threshold of success
   reports — and that aggregation is out of scope (the model is scoped to
   `engine/multisig/src/client/`).

So K7 is not an invariant. The quorum-coupling constraint is kept as part of the
model, divergence is tracked by a **witness** confirming the model can still
reach it, and sub-quorum finalisation is recorded here as a deliberate
non-property with its scope boundary stated.

K4 targets a bug class known to be real. `VerifyComplaintsBroadcastStage7`
already carries a fix for it: complaints from non-receiving participants are
discarded, because acting on one would force a blamed party to reveal a share at
a bogus index, fail verification in stage 9, and wrongly attribute a possibly
honest party. The model should rediscover that requirement rather than assume it.

K6 is the abstract shadow of review finding #1. Tracking committed degree as an
integer lets the model check that agreed commitments plus the length check rule
out the "keygen finalises, first signing fails for everyone" outcome, without
modelling any curve arithmetic.

## Verification

Tooling: `@informalsystems/quint` (npm, tested at 0.32.0) and Apalache (tested
at 0.56.1, auto-downloaded into `~/.quint` on first `quint verify`; requires a
JDK). A prototype confirmed the approach end to end before this plan was
written:

| Property | Result at n=4/f=1 | Time |
| --- | --- | --- |
| L1 NoFalseBlame | verified exhaustively | 144 s |
| L2 ValueAgreement | verified exhaustively | 159 s |
| L4 OutcomeAgreement (original form) | **falsified**, counterexample above | 14 ms |

**Encoding constraint discovered during prototyping.** Apalache rejects
`setOfMaps(D, C).oneOf()` with *"Trying to expand a set of functions. This will
blow up the solver."* Adversary choices must therefore be encoded as
`Set[record].powerset().oneOf()` plus explicit well-formedness constraints,
not as sets of maps. The simulator (`quint run`) accepts either form, so this
only surfaces at `quint verify` time — it must be got right from the first
module or every model has to be rewritten later.

Configurations:

- lemma layer: exhaustive at n=4/f=1 and n=7/f=2;
- ceremony layer: exhaustive at n=4/f=1 and n=5/f=1;
- handover splits: `sharing = {1,2,3}`, `receiving = {3,4,5}` at n=5/f=1 — note
  this is deliberately tight, since a quorum over three sharing parties is two,
  leaving no slack if the Byzantine party is a sharer;
- randomised `quint run` beyond the exhaustive envelope for bug-hunting.

Where a configuration does not fit an exhaustive check, use bounded-depth
verification and record the bound in `harness.qnt` alongside the run, so the
coverage claim stays honest.

### Counterexamples become Rust tests

`client/helpers.rs` drives ceremonies at message granularity —
`gather_outgoing_messages`, `distribute_messages`,
`distribute_messages_with_non_sender`, `run_stage_with_non_sender`. A Quint
counterexample is a per-stage record of who sent what to whom, which maps onto
those helpers directly. Every property violation the model finds should land in
`keygen/tests.rs` as a regression test, so the model's findings survive
independently of whether anyone re-runs the model.

## Build order

Each phase ends with something checkable.

1. **Lemma.** `types.qnt` + `broadcast.qnt`; L1–L6 verified at n=4/f=1, then
   n=7/f=2. This phase alone answers the review's "pressure-test the quorum
   math" request and is the milestone to reach before committing to the rest.
2. **Ceremony skeleton.** `oracle.qnt` + keygen stages 0–5; K1, K2, K5.
   Includes the seam cross-check: instantiate `keygen.qnt` against the concrete
   `broadcast.qnt` at n=4 to confirm the idealisation is no stronger than what
   L1–L6 prove.
3. **Blame sub-protocol.** Stages 6–9; K3.
4. **Handover.** Parameterise sharing/receiving sets and index remapping;
   K4, K6.
5. **Integration.** CI wiring and the counterexample-to-Rust workflow.

## Risks

**State-space blowup** is the main technical risk. Mitigations: keep `Value` an
uninterpreted type with two or three inhabitants; keep each stage's adversary
choice space as small as still covers the attack classes; prefer bounded-depth
verification over dropping a configuration entirely.

**The oracle seam** is the main methodological risk, addressed by the phase-2
cross-check described above.

**CI cost.** Apalache runs are slow. The realistic arrangement is small
configurations on pull requests that touch `engine/multisig/`, with the larger
configurations nightly. A model that is too slow to run is a model that stops
being run.
