// Instantiations of `keygen` with concrete `const` values.
//
// `keygen.qnt` declares `SHARING`, `RECEIVING`, `FILTER_NON_RECEIVER_COMPLAINTS`
// and `ENFORCE_COEFF_LENGTH` as `const`, so it cannot be run directly - every
// check targets one of the modules below via `--main`.

module plain {
  import types.* from "./types"
  import keygen(
    SHARING = PARTIES,
    RECEIVING = PARTIES,
    FILTER_NON_RECEIVER_COMPLAINTS = true,
    ENFORCE_COEFF_LENGTH = true
  ).* from "./keygen"

  // Proves `Done` is reachable via `step`'s actual transition logic
  // (`stepWith`, the deterministic core `step` draws its nondet arguments
  // for) - not inferred from random sampling. After wiring in the
  // blame-response-completeness check (F3), a uniformly random walk only
  // reaches `Done` in about 1 in 45000 traces, because it requires agreement
  // at all four verify stages simultaneously - correct model behaviour, but
  // the wrong instrument for a reachability claim. This test drives the
  // happy path directly: every party agrees at every verify stage, no bad
  // shares, no complaints, no blame, honest coefficient lengths throughout.
  // It is deterministic (fixed arguments, no `nondet`) and runs in
  // milliseconds - it does not depend on sampling luck.
  //
  // lenHi = Set(), lenLo = BYZ gives every Byzantine party the honest
  // coefficient-commitment length too (KEY_THRESHOLD + 1) - see the
  // `newLens` branching in `stepWith` - so nobody fails the length check
  // either. The ceremony needs exactly 10 stage transitions to reach
  // `Finished` from `PubkeyShares0` (see the README's --max-steps note).
  run doneReachableTest = {
    init
      .then(10.reps(_ => stepWith(
        true, PARTIES, Set(), Set(), Set(), Set(), Set(), Set(), BYZ)))
      .expect(HONEST.forall(k =>
        match result.get(k) {
        | Done(m) => m == KEY_A
        | Failed(_) => false
        | Running => false
        }))
  }
}

// Handover with BYZ = {4} as a SHARER THAT IS NOT A RECEIVER.
//
// This configuration is not arbitrary and must not be "tidied". The bug this
// task exists to catch only arises for a non-receiving participant: it holds no
// incoming shares, so any complaint from it is necessarily dishonest. If the
// Byzantine party is also a receiver, its complaints are legitimate, the bug is
// unreachable, and the negative control below passes silently - which reads as
// success while proving nothing. Verified: with RECEIVING = {3,4} (party 4 a
// receiver) the control reports [ok]; with the split below it reports
// [violation], as it must.
module handover {
  import types.* from "./types"
  import keygen(
    SHARING = Set(2, 3, 4),
    RECEIVING = Set(1, 2, 3),
    FILTER_NON_RECEIVER_COMPLAINTS = true,
    ENFORCE_COEFF_LENGTH = true
  ).* from "./keygen"

  // K4 is K1 under the handover configuration - the property is the same, the
  // configuration is what makes it a distinct check.
  val K4_HandoverNoFalseBlame = K1_NoHonestBlamed

  // K7 is a WITNESS, not an invariant. Stating it as "no honest party may
  // finalise while another failed" is false of the real protocol: a minority of
  // honest parties aborting while the quorum completes is correct threshold
  // behaviour. Whether a locally-finalised key is ever used is decided outside
  // this model, by the State Chain requiring a threshold of success reports.
  // What the model must show is that it can still REACH divergence - a model
  // that cannot has lost the adversarial power the L4 counterexample proved is
  // real. Must be witnessed in > 0 traces.
  //
  // wCeremonyDiverged / wCeremonyDone / wCeremonyBlamed are NOT redeclared here:
  // they are inherited from `keygen` through the `import ... .*` above, and
  // redeclaring them is a duplicate-name error (QNT101).
}

// Negative control: the non-receiver complaint filter switched OFF. K4 MUST
// fail here. If it passes, the model cannot see a bug the Rust already fixes
// (VerifyComplaintsBroadcastStage7) and is not trustworthy for finding others.
module handoverUnfixed {
  import types.* from "./types"
  import keygen(
    SHARING = Set(2, 3, 4),
    RECEIVING = Set(1, 2, 3),
    FILTER_NON_RECEIVER_COMPLAINTS = false,
    ENFORCE_COEFF_LENGTH = true
  ).* from "./keygen"

  val K4_MustFailHere = K1_NoHonestBlamed
}

// Negative control: the coefficient-length check switched OFF. K6 MUST fail
// here - this is review finding #1 reproduced.
module plainNoCoeffCheck {
  import types.* from "./types"
  import keygen(
    SHARING = PARTIES,
    RECEIVING = PARTIES,
    FILTER_NON_RECEIVER_COMPLAINTS = true,
    ENFORCE_COEFF_LENGTH = false
  ).* from "./keygen"

  val K6_MustFailHere = K6_KeyConsistency
}
