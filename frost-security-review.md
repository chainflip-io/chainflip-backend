# FROST security recommendations vs. the Chainflip codebase

**Date:** 2026-05-15
**Scope reviewed:** `engine/multisig/src/client/{signing,keygen,common}/` and
`crypto.rs`, plus git history back through the 2023 refactors.

## Headline

Four of the five items are in good shape and largely already mitigated. **One is
a substantiated concern that directly matches the thing flagged for double-checking
("is the check still in place after refactors since 2023?") — and the answer
appears to be: it was weakened by a May-2023 refactor and there is no equivalent
replacement.** Details below.

---

## 1. Trail of Bits Pedersen-DKG coefficient-length check — ⚠️ LIKELY REGRESSED, needs expert confirmation

This is the one to act on.

**What ToB credited Chainflip with (pre-2024):** a per-ceremony check on the
number of coefficient commitments. It existed. Before PR #3248 (`f9204f5d17`,
"Avoid Wasteful Deserialization in Ceremonies", May 2023) the keygen
message-size validator did:

```rust
// engine/.../keygen/keygen_data.rs  (pre-#3248)
KeygenData::CoeffComm3(message) => {
    // ...it should never exceed the number of parties
    message.get_commitments_len() <= num_of_parties   // get_commitments_len() == commitments.0.len()
}
```

That is a *per-ceremony* bound on the actual EC-point coefficient count, applied
to **every received commitment vector** before processing — exactly the
ToB-class defense.

**What it is now** (`keygen/keygen_data.rs:90`):

```rust
KeygenData::CoeffComm3(message) => message.payload.len() <= MAX_COEFF_COMM_3_SIZE,
```

`MAX_COEFF_COMM_3_SIZE` (`keygen_detail.rs:320-324`) is derived from
`MAX_AUTHORITIES` (the global, network-wide cap ~150), **not** the current
ceremony's party/threshold count. The refactor replaced a per-ceremony
*element-count* check with a global *byte-size* cap. In a small ceremony (say 10
signers, threshold ~6, honest vector length 7) a malicious participant can
broadcast up to ~`MAX_COEFFICIENTS` (~100) coefficients and still pass
`is_data_size_valid`.

**Every downstream consumer was traced to see if the per-ceremony exactness is
re-imposed. It is not:**

- `validate_commitments` (`keygen_detail.rs:394-463`): checks the ZKP (only
  against `commitments.0[0]`) and the stage-1 hash commitment. The hash
  commitment binds the attacker to whatever-length vector *they themselves chose*
  (re-hash of the revealed vector trivially matches) — it is **not** a length
  defense. No length/degree check here, in either the old or current code.
- `verify_share` (`keygen_detail.rs:277-284`): evaluates the **full**
  `commitments.0.iter()` vector.
- `derive_local_pubkeys_for_parties` (`keygen_detail.rs:509-514`): evaluates
  only `(0..=threshold)`, i.e. **silently truncates** to degree `threshold` and
  **panics (index OOB)** if the vector is shorter than `threshold+1`.

This asymmetry produces two concrete, in-code failure modes for a single
malicious keygen participant whose ZKP-on-`c0` and hash-commitment are valid:

- **Over-long vector (degree raised):** shares are computed with the full
  high-degree polynomial, so honest `verify_share` *passes* (no complaint),
  keygen *finalizes*. But pubkey derivation truncates → derived `y_i` ≠ `G·x_i`
  for that contribution → the resulting aggregate key is internally inconsistent
  and **the first signing attempt fails for everyone, with no in-keygen
  attribution**. This is precisely the ToB outcome ("silently raises the
  threshold / makes future signing impossible").
- **Too-short / empty vector:** passes ZKP+hash+`verify_share`, then
  `derive_local_pubkeys_for_parties` (or `validate_commitments`'s
  `c.commitments.0[0]` for an empty vec) does a **direct out-of-bounds index →
  panic** on honest nodes at finalize — a single-participant keygen
  griefing/DoS.

**Calibration:** This is *not* an assertion of a confirmed live exploit. ToB's
2024 "not vulnerable" verdict was rendered against code that still contained the
per-ceremony bound; operational factors (permissioned staked validator set,
keygen-failure re-tries, slashing) reduce real-world impact; and the old
`<= num_of_parties` bound was itself only a loose upper bound (degree ≤ n−1),
not an exact `== t+1`, so the protocol always also leaned on the
hash-commit/ZKP/verify_share structure. But the specific question that was to be
verified — *"is the check still in place after refactors since 2023?"* — has a
clear answer from code + git: **the per-ceremony coefficient-count check was
removed by PR #3248 and no equivalent per-ceremony length/degree validation
exists downstream.** This warrants urgent confirmation by the cryptography team.

**Recommended actions:**

- Re-introduce an explicit, per-ceremony check that every received
  `DKGUnverifiedCommitment.commitments.0` has length **exactly** `threshold + 1`
  (and `>= 1`), rejecting/attributing before `derive_aggregate_pubkey`/finalize,
  rather than relying on the global byte cap.
- Add a keygen unit test that injects (a) an over-long and (b) a
  too-short/empty coefficient vector from one participant and asserts the
  ceremony aborts **with that party attributed**, not "produces a key" or
  "panics". The existing tests only cover *value* corruption
  (`corrupt_primary_coefficient`/`corrupt_secondary_coefficient`), never
  *length*.

## 2. CertiK unidentifiable-abort / inconsistent nonce commitments (Sept 2025) — ✅ ALREADY MITIGATED

This is the item the report flagged as most actionable; the design already
implements CertiK's proposed mitigation, and has for years.

The FROST signing flow (`signing/signing_stages.rs`) is:

1. `AwaitCommitments1` — broadcast nonce commitments `(d_pub, e_pub)`
   (`SecretNoncePair::sample_random`).
2. **`VerifyCommitmentsBroadcast2`** — every node **re-broadcasts the full set
   of commitments it received**, then runs `verify_broadcasts_non_blocking`
   (`common/broadcast_verification.rs`). Only the consensus-agreed commitment
   set proceeds.
3. `LocalSigStage3` — signature share computed **only after** step 2, and
   crucially the binding values/group commitment are derived from
   `verified_commitments` (the post-consensus set, `signing_stages.rs:203-231`),
   so every honest node uses an identical view.
4. `VerifyLocalSigsBroadcastStage4` — same echo-and-verify before aggregation.

This is exactly CertiK's "additional round verifying that all received nonce
commitments are consistent across the network, aborting on inconsistency".
`verify_broadcasts` attributes inconsistency to the **broadcaster whose echoed
values lack quorum** (`find_frequent_element` per sender,
`BroadcastFailureReason::Inconsistency`), not the honest receivers — see the
explicit comment at `broadcast_verification.rs:96-99` and the
`fail_from_inconsistent_broadcast` test. So an attacker who sends different
commitments to different honest parties gets *themselves* reported; honest nodes
cannot be framed. The CertiK assumption ("RFC 9591 leaves this consistency check
to the implementation, gossip layers may skip it") does **not** apply here. No
exposure.

## 3. CRYPTO 2025 adaptive-security results — ℹ️ INFORMATIONAL, no code-level tweak that worsens it

The key shares are standard FROST: `KeyShare { y, x_i }` with `pk_i = G·x_i`
over a Shamir polynomial (`keygen_detail.rs`, `signing_detail.rs`). There is
**no non-standard key-share structure tweak**, so the "FROST variants with
modified key shares may have weaker adaptive guarantees" caveat does not
specifically apply — Chainflip inherits exactly the reference FROST
adaptive-security posture, neither better nor worse. This remains a threat-model
question (stable staked operator set vs. real-time adaptive bribery of >t−1
nodes), not a code defect. No action beyond awareness.

## 4. Implementation pitfalls — ✅ MOSTLY GOOD (one weak point already covered by #1)

- **Nonce randomness (not deterministic):** ✅ Production keygen and signing
  both seed the ceremony RNG with `Rng::from_entropy()` (`client.rs:229`,
  `client.rs:355`); `Rng = rand::rngs::StdRng` (ChaCha-family CSPRNG,
  `crypto.rs:98`). `SecretNoncePair::sample_random` draws `d, e` from it
  (`signing_detail.rs:45-53`). Not derived from any deterministic function of
  ceremony inputs.
- **Single-use + deletion:** ✅ Fresh nonces per ceremony; explicitly zeroized
  in `LocalSigStage3::init` after the share is produced
  (`signing_stages.rs:291-297`, comment cites Fig. 3 step 6). `SecretNoncePair`
  derives `Zeroize`.
- **Snapshot-rollback nonce reuse:** ✅ Not applicable — `SecretNoncePair`
  appears only in `signing_detail.rs`/`signing_stages.rs`, held in-memory in the
  stage struct; there is no nonce/ceremony-state persistence layer (`key_store`
  persists key shares only). Even a VM rollback re-running the same ceremony
  re-seeds from OS entropy, so nonces differ.
- **DKG Schnorr proof-of-knowledge (anti-rogue-key):** ✅ Present —
  `generate_zkp_of_secret` (`keygen_detail.rs:126-142`) and `is_valid_zkp`
  enforced in `validate_commitments` (`keygen_detail.rs:437`). Round-1 hash
  commitment + ZKP + the high-degree non-degeneracy check
  (`check_high_degree_commitments`) are all in place.
- **Domain separation:** ✅ Reasonable — the DKG challenge binds
  `pubkey ‖ commitment ‖ index ‖ HashContext` (`generate_dkg_challenge`, with
  `HashContext` for replay protection); the signing binding value `gen_rho_i` is
  prefixed with a `b"I"` tag and binds index ‖ msg ‖ ordered per-party `(d,e)`
  (`signing_detail.rs:92-128`). Both use Blake2b-256.
- The one real weakness in this category is the **missing per-ceremony
  coefficient-vector length validation**, which is finding #1.

## 5. Suggested next steps

- **Prioritize #1.** It is the concrete, code-substantiated item and it is
  precisely the regression class the source warned about. The fix is small (an
  exact length check) but the correctness reasoning deserves a cryptographer's
  eyes plus the regression tests described above.
- For #2 a future auditor can be given a strong, specific brief: "we already
  implement the CertiK mitigation via the `VerifyCommitmentsBroadcast2`
  echo+`verify_broadcasts` round; please pressure-test the *attribution* logic
  in `broadcast_verification.rs` (the `threshold_for_broadcast_verification` /
  `find_frequent_element` quorum math) rather than the existence of the check."
- #3 is a threat-model conversation, not a code review.

---

*No code changes were made — this is an assessment only.*
