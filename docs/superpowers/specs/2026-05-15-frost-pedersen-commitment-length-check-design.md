# FROST DKG: per-ceremony Pedersen commitment-vector length check

**Date:** 2026-05-15
**Status:** Approved (design); implementation pending
**Related:** `frost-security-review.md` finding #1 (Trail-of-Bits Pedersen-DKG
coefficient-length check — regressed by PR #3248, May 2023)

## Problem

`KeygenData::CoeffComm3` validation (`engine/multisig/src/client/keygen/keygen_data.rs:90`)
only enforces a global byte-size cap:

```rust
KeygenData::CoeffComm3(message) => message.payload.len() <= MAX_COEFF_COMM_3_SIZE,
```

`MAX_COEFF_COMM_3_SIZE` (`keygen_detail.rs:320-324`) is derived from
`MAX_AUTHORITIES` (~150, the network-wide cap), **not** the current ceremony's
party/threshold count. PR #3248 ("Avoid Wasteful Deserialization in Ceremonies")
replaced a per-ceremony *element-count* check (`get_commitments_len() <=
num_of_parties`) with this global *byte-size* cap, and no equivalent
per-ceremony length/degree validation was re-imposed downstream.

Consequence — a single malicious keygen participant whose ZKP-on-`c0` and
hash-commitment are internally consistent can broadcast a wrong-length
coefficient vector:

- **Over-long (degree raised):** honest `verify_share` passes (evaluates the
  full vector), keygen finalizes, but `derive_local_pubkeys_for_parties`
  truncates at `(0..=threshold)` → derived `y_i ≠ G·x_i` → aggregate key
  internally inconsistent → **first signing attempt fails for everyone, no
  in-keygen attribution**.
- **Too-short / empty:** passes ZKP+hash+`verify_share`, then a direct
  out-of-bounds index (`c.commitments.0[0]` in `validate_commitments`,
  `[k as usize]` in `derive_local_pubkeys_for_parties`) → **panic on honest
  nodes at finalize** — single-participant griefing/DoS.

The hash-commitment is *not* a length defense: a malicious node commits to its
wrong-length vector from the start, so `HashComm1` and `CoeffComm3` are
mutually consistent. The ZKP only binds `commitments.0[0]`.

Existing keygen tests only cover *value* corruption
(`corrupt_primary_coefficient` / `corrupt_secondary_coefficient`), never
*length*.

## Decisions (locked)

1. **Scope:** re-introduce the per-ceremony length check **and** add regression
   tests that guard it (security review's recommendation).
2. **Check rule:** exact — reject unless `len == threshold + 1`. Honest nodes
   always produce exactly `threshold + 1` commitments (`[secret] ++
   coefficients`, where `coefficients.len() == threshold`), in both keygen and
   key-handover. Exactness also closes the degree-raising attack that a loose
   `<= num_of_parties` bound would still permit. Flagged for crypto-team
   confirmation in code/PR.
3. **Coverage:** regression tests on **both** the normal keygen path and the
   key-handover/resharing path (the latter derives expected length from the
   *new-key* threshold, not the sharing-party count — the subtle case).

## Fix

`validate_commitments` (`keygen_detail.rs:394`) gains a parameter carrying the
expected coefficient-vector length (the per-ceremony / new-key
`threshold + 1`). As the **first** check in the per-party `filter_map` closure
— before any `c.commitments.0[0]` access (resharing first-commitment check at
line ~424, `generate_dkg_challenge` at line ~431):

```rust
if c.commitments.0.len() != expected_len {
    warn!(
        from_id = validator_mapping.get_id(*idx).to_string(),
        expected_len, actual_len = c.commitments.0.len(),
        "Invalid commitment vector length"
    );
    return Some(*idx);
}
```

- `expected_len = threshold + 1`; since `threshold >= 1` this enforces both
  exactness and non-emptiness, making every later `commitments.0[k]` / `[0]`
  access in `validate_commitments`, `derive_aggregate_pubkey`, `verify_share`,
  and `derive_local_pubkeys_for_parties` provably in-bounds.
- Offending parties flow into the existing `invalid_idxs` set →
  `Err((invalid_idxs, KeygenFailureReason::InvalidCommitment))` →
  `StageResult::Error` → ceremony aborts **with attribution**, before
  `derive_aggregate_pubkey`.

### Why this placement

`validate_commitments` is the single point that already does per-party
attribution, returns `(BTreeSet<AuthorityCount>, KeygenFailureReason)`, and
runs in `VerifyCommitmentsBroadcast4::process()` before
`derive_aggregate_pubkey`. Centralising the check here (rather than a separate
pre-step, or strengthening `is_data_size_valid`) keeps all commitment
validation cohesive and is the only spot that satisfies "abort with
attribution": a failure in `is_data_size_valid` is *silently dropped with no
attribution* (ceremony stalls to timeout, attacker never blamed).

### Call sites threading the new parameter

Only two real call sites of `validate_commitments`:

- `keygen_stages.rs:486` (production, `VerifyCommitmentsBroadcast4::process()`)
  — passes `keygen_common.sharing_params.key_params.threshold + 1`. This is
  already the *new-key* threshold for both keygen
  (`ThresholdParameters::from_share_count(all_idxs.len())`) and key-handover
  (`from_share_count(receiving_participants.len())`), so one expression is
  correct for both paths.
- `keygen_detail.rs:622` (the `keygen_sequential` unit test) — passes its own
  `params.threshold + 1`.

`genesis::generate_key_data_detail` constructs `DKGCommitment` directly and
does not call `validate_commitments`, so it is unaffected.

The global `MAX_COEFF_COMM_3_SIZE` byte cap (`keygen_data.rs:90`) is **kept**
as a cheap first-line DoS guard; it is simply no longer the only defense.

## Regression tests

### Test-only helpers (on `DKGUnverifiedCommitment<P>`, `keygen_detail.rs`,
alongside `corrupt_primary_coefficient`)

- `lengthen_commitments(&mut self, extra, rng)` — append `extra` random points
  (degree-raising; `commitments.0[0]` untouched so ZKP stays valid).
- `truncate_commitments(&mut self, new_len)` — keep `new_len >= 1` points
  (too-short but ZKP still valid on `[0]`).
- `clear_commitments(&mut self)` — empty vector (the panic case; there is no
  `[0]`, so the fix's length check must run before any `[0]` access — see
  Fix).

### Attack faithfulness

The malicious node must broadcast a *consistent* `(HashComm1, CoeffComm3)`
pair of the wrong length, identically to all recipients:

- Recompute the bad node's `HashComm1` as
  `generate_hash_commitment(&mutated_commitment)` and inject it at the
  `HashComm1` stage (so `is_valid_hash_commitment` passes — proving it is the
  *length* check, not the hash check, that rejects).
- Inject `DelayDeserialization::new(&mutated_commitment)` at the `CoeffComm3`
  stage.
- Send both identically to every recipient so `VerifyHashComm2` /
  `VerifyCoeffComm4` broadcast-verification pass and the failure is attributed
  at `validate_commitments` as `InvalidCommitment`, not as
  `BroadcastFailure(Inconsistency, …)`.

Assertion: `ceremony.complete_with_error(&[bad_account_id],
KeygenFailureReason::InvalidCommitment)`.

### Cases

Keygen path (`KeygenCeremonyRunnerEth::new_with_default()`):

1. over-long by 1 (degree-raising)
2. too-short by 1 (non-empty)
3. empty (the pre-fix panic case)

Key-handover/resharing path (the `key_handover` test module:
`prepare_handover_test`, `KeygenCeremonyRunner::<BtcSigning>`,
`request_key_handover`, stage flow `PubkeyShares0 → HashComm1 →
VerifyHashComm2 → CoeffComm3 → VerifyCoeffComm4 → …`): at minimum over-long and
empty, to exercise the new-key-threshold expected-length derivation.

### Red/green

On pre-fix code: over-long → ceremony finalizes (so `complete_with_error`
fails because the ceremony `complete()`s), empty → panic (test fails via
panic). After the fix: all new tests green; existing tests
(`should_report_on_invalid_hash_comm`,
`should_report_on_invalid_zkp_in_coeff_comm`, `keygen_sequential`,
`check_comm3_max_size`, key_handover suite) remain green.

## Verification

- `cargo nextest run -p multisig` (keygen + key_handover + `check_comm3_max_size`)
- `cargo check -p multisig` / `cargo cf-clippy` on the engine
- Confirm the two `validate_commitments` call sites compile with the new
  parameter and the existing corruption/handover tests still pass.

## Out of scope

- Changing or removing the global `MAX_COEFF_COMM_3_SIZE` byte cap.
- Cryptographic re-derivation of the exact-vs-loose bound — exact `==
  threshold + 1` is implemented; correctness reasoning is flagged for the
  crypto team in the PR.
