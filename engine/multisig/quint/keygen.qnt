module keygen {
  import types.* from "./types"
  import oracle.* from "./oracle"

  type Stage =
    | PubkeyShares0 | HashCommitments1 | VerifyHashCommitments2
    | CoefficientCommitments3 | VerifyCommitments4 | SecretShares5
    | Complaints6 | VerifyComplaints7 | BlameResponses8 | VerifyBlameResponses9
    | Finished

  type CeremonyOutcome = Running | Done(Party -> Vote) | Failed(Set[Party])

  pure def nextStage(s: Stage): Stage =
    match s {
    | PubkeyShares0 => HashCommitments1
    | HashCommitments1 => VerifyHashCommitments2
    | VerifyHashCommitments2 => CoefficientCommitments3
    | CoefficientCommitments3 => VerifyCommitments4
    | VerifyCommitments4 => SecretShares5
    | SecretShares5 => Complaints6
    | Complaints6 => VerifyComplaints7
    | VerifyComplaints7 => BlameResponses8
    | BlameResponses8 => VerifyBlameResponses9
    | VerifyBlameResponses9 => Finished
    | Finished => Finished
    }

  // Stages that consume an echo-broadcast oracle result.
  pure def isVerifyStage(s: Stage): bool =
    s == VerifyHashCommitments2 or s == VerifyCommitments4
      or s == VerifyComplaints7 or s == VerifyBlameResponses9

  run stageOrderTest = all {
    assert(nextStage(PubkeyShares0) == HashCommitments1),
    assert(nextStage(VerifyCommitments4) == SecretShares5),
    assert(nextStage(VerifyBlameResponses9) == Finished),
    assert(nextStage(Finished) == Finished),
  }

  var stage: Party -> Stage
  var result: Party -> CeremonyOutcome
  var shareValid: Set[{ from: Party, to: Party }]
  var complaints: Set[{ by: Party, about: Party }]
  var revealed: Set[{ by: Party, about: Party, ok: bool }]
  // 0 and KEY_THRESHOLD are too short; KEY_THRESHOLD + 2 is too long;
  // KEY_THRESHOLD + 1 is the honest length.
  var coeffLen: Party -> int

  pure val ALL_PAIRS: Set[{ from: Party, to: Party }] =
    tuples(PARTIES, PARTIES).map(t => { from: t._1, to: t._2 })

  action init = all {
    stage' = PARTIES.mapBy(_ => PubkeyShares0),
    result' = PARTIES.mapBy(_ => Running),
    shareValid' = ALL_PAIRS,
    complaints' = Set(),
    revealed' = Set(),
    coeffLen' = PARTIES.mapBy(_ => KEY_THRESHOLD + 1),
  }

  // Two distinct candidate keys, so K2 (no two honest parties finish with
  // different keys) is not vacuously true.
  pure val KEY_A: Party -> Vote = PARTIES.mapBy(i => Sent(i))
  pure val KEY_B: Party -> Vote = PARTIES.mapBy(i => Sent(i + 1))

  // Handover splits parties into sharers (hold the current key material and
  // send shares of the new key) and receivers (hold the new key material and
  // verify/complain/finalise). Supplied per-instantiation by harness.qnt.
  // Plain keygen: SHARING = RECEIVING = PARTIES.
  const SHARING: Set[Party]
  const RECEIVING: Set[Party]

  // Complaints from non-receiving participants must be discarded. A non-receiver
  // processes no shares, so any complaint from one is necessarily dishonest;
  // acting on it forces the blamed party to reveal a share at an index that
  // then fails stage-9 verification, wrongly attributing a possibly honest node.
  // `const` so harness.qnt can instantiate the unfixed variant as a permanent
  // negative control.
  const FILTER_NON_RECEIVER_COMPLAINTS: bool

  pure def admissibleComplaints(
    cs: Set[{ by: Party, about: Party }],
    receiving: Set[Party]
  ): Set[{ by: Party, about: Party }] =
    if (FILTER_NON_RECEIVER_COMPLAINTS) cs.filter(c => receiving.contains(c.by)) else cs

  // One stage transition.
  //
  // Each party gets its OWN oracle result: the verified L4 counterexample shows
  // a Byzantine party can equivocate in round 1 and tie-break in round 2, so
  // some honest parties agree while others fail. A single shared result would
  // make K2 vacuous.
  //
  // CRITICAL: every choice is decoded into a valid configuration. Do NOT draw
  // freely and add constraints inside `all { }` - that disables the transition
  // rather than restricting the choice, and the cost of completing a trace then
  // grows about 2.6x per step (measured: 84s at 8 steps, over 9 minutes at 10).
  // The oracle contract is honoured structurally instead:
  //   - ONE agreed key is chosen per step, so two honest parties can never land
  //     on different agreed maps (this is L2, which broadcast.qnt proved);
  //   - blame is always BYZ - non-empty and disjoint from HONEST (this is L1);
  //   - agreement is only offered while a quorum is still participating, because
  //     every stage collects from all participants and the rest time out once
  //     too many have aborted.
  action step = {
    nondet keyIsA = Set(false, true).oneOf()
    nondet agreedSet = PARTIES.powerset().oneOf()
    nondet blamedSet = PARTIES.powerset().oneOf()
    nondet byzBadShares =
      tuples(BYZ, PARTIES).map(t => { from: t._1, to: t._2 }).powerset().oneOf()
    nondet byzComplaints =
      tuples(BYZ, PARTIES).map(t => { by: t._1, about: t._2 }).powerset().oneOf()
    // Stage 8: a blamed party reveals the share it sent to each complainant.
    // An honest party reveals exactly those, truthfully. A Byzantine party may
    // reveal any subset, with any validity.
    // Revealed-at-all and valid-or-not, as two characteristic subsets, so a
    // draw can never assert both ok:true and ok:false for the same pair.
    nondet byzRevealMade = tuples(BYZ, PARTIES).powerset().oneOf()
    nondet byzRevealOk = tuples(BYZ, PARTIES).powerset().oneOf()
    // Four length choices per Byzantine party via two characteristic subsets.
    nondet lenHi = BYZ.powerset().oneOf()
    nondet lenLo = BYZ.powerset().oneOf()

    val stillRunning = PARTIES.filter(p => result.get(p) == Running)
    val canAgree = stillRunning.size() > T
    val theKey = if (keyIsA) KEY_A else KEY_B
    val res = PARTIES.mapBy(k =>
      if (canAgree and agreedSet.contains(k)) OkAgreed(theKey)
      else if (blamedSet.contains(k)) OkAttributed(BYZ)
      else OkFailed)

    // Honest k complains about exactly the senders whose share to k was invalid.
    val honestComplaints =
      tuples(HONEST, PARTIES)
        .filter(t => not(shareValid.contains({ from: t._2, to: t._1 })))
        .map(t => { by: t._1, about: t._2 })
    val byzReveals = byzRevealMade.map(t =>
      { by: t._1, about: t._2, ok: byzRevealOk.contains(t) })
    val honestReveals =
      admissibleComplaints(complaints, RECEIVING)
        .filter(c => HONEST.contains(c.about))
        .map(c => { by: c.about, about: c.by,
                    // A reveal at a non-receiver's index is evaluated at an
                    // index that party never held a share for, so it fails
                    // verification - which is how an honest node gets blamed.
                    ok: RECEIVING.contains(c.by)
                        and shareValid.contains({ from: c.about, to: c.by }) })
    // Attribution for VerifyBlameResponses9: whoever produced a failing reveal
    // (in `revealed`, populated during the previous step's BlameResponses8) is
    // blamed. Reads the already-committed `revealed`, NOT this step's fresh
    // `honestReveals.union(byzReveals)` draw - same one-step-later pattern as
    // `badLenParties` below.
    val badRevealers = revealed.filter(r => not(r.ok)).map(r => r.by)
    val newLens = PARTIES.mapBy(i =>
      if (not(BYZ.contains(i))) KEY_THRESHOLD + 1
      else if (lenHi.contains(i))
        (if (lenLo.contains(i)) 0 else KEY_THRESHOLD)
      else
        (if (lenLo.contains(i)) KEY_THRESHOLD + 1 else KEY_THRESHOLD + 2))
    // Checked against the COMMITTED lengths in `coeffLen`, NOT against this
    // step's fresh `newLens` draw. The commitment is broadcast at stage 3 and
    // validated at stage 4, one step later. Reading `newLens` here tests a
    // value that was never committed, and K6 then fails even with the check
    // enabled - a trap this plan hit once already.
    val badLenParties = PARTIES.filter(i => not(commitmentAccepted(coeffLen.get(i))))

    all {
      stage' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) stage.get(k)
        else if (stage.get(k) == VerifyCommitments4 and badLenParties != Set())
          stage.get(k)
        else if (stage.get(k) == VerifyBlameResponses9 and badRevealers != Set())
          stage.get(k)
        else if (isVerifyStage(stage.get(k)) and
                 (match res.get(k) {
                  | OkAgreed(_) => false
                  | OkAttributed(_) => true
                  | OkFailed => true
                 })) stage.get(k)
        else nextStage(stage.get(k))),
      result' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) result.get(k)
        else if (stage.get(k) == VerifyCommitments4 and badLenParties != Set())
          Failed(badLenParties)
        else if (stage.get(k) == VerifyBlameResponses9 and badRevealers != Set())
          Failed(badRevealers)
        else if (isVerifyStage(stage.get(k)))
          match res.get(k) {
          | OkAgreed(m) => if (nextStage(stage.get(k)) == Finished) Done(m) else Running
          | OkAttributed(bad) => Failed(bad)
          | OkFailed => Failed(Set())
          }
        else if (nextStage(stage.get(k)) == Finished) Done(KEY_A)
        else Running),
      shareValid' = if (PARTIES.exists(k => stage.get(k) == SecretShares5))
                      ALL_PAIRS.exclude(byzBadShares) else shareValid,
      complaints' = if (PARTIES.exists(k => stage.get(k) == Complaints6))
                      honestComplaints.union(byzComplaints) else complaints,
      revealed' = if (PARTIES.exists(k => stage.get(k) == BlameResponses8))
                    honestReveals.union(byzReveals) else revealed,
      coeffLen' = if (PARTIES.exists(k => stage.get(k) == CoefficientCommitments3))
                    newLens else coeffLen,
    }
  }

  // K1: no honest party is blamed by any honest party. THE headline property.
  val K1_NoHonestBlamed = HONEST.forall(k =>
    match result.get(k) {
    | Failed(bad) => bad.intersect(HONEST) == Set()
    | Done(_) => true
    | Running => true
    })

  // K2: no two honest parties finish with different keys.
  val K2_NoConflictingOutcome = tuples(HONEST, HONEST).forall(p =>
    match result.get(p._1) {
    | Done(k1) => match result.get(p._2) { Done(k2) => k1 == k2 | Failed(_) => true | Running => true }
    | Failed(_) => true
    | Running => true
    })

  // K5: no honest party is stuck Running once it reaches Finished.
  val K5_Termination = HONEST.forall(k =>
    stage.get(k) != Finished or result.get(k) != Running)

  // K3: blame must be productive as well as safe - a non-empty reported set
  // contains at least one Byzantine party, or an adversary can force endless
  // unattributed retries.
  val K3_AttributionProgress = HONEST.forall(k =>
    match result.get(k) {
    | Failed(bad) => bad == Set() or bad.intersect(BYZ) != Set()
    | Done(_) => true
    | Running => true
    })

  // K6: a ceremony can never finalise with a wrong-length commitment in play.
  // This is review finding #1 stated as an invariant: "keygen finalises, first
  // signing fails for everyone" must be unreachable.
  val K6_KeyConsistency = HONEST.forall(k =>
    match result.get(k) {
    | Done(_) => PARTIES.forall(i => commitmentLengthValid(coeffLen.get(i), KEY_THRESHOLD))
    | Failed(_) => true
    | Running => true
    })

  // Witnesses. These are not properties to hold - they are coverage checks.
  // Each must be reached in > 0 traces, or the run's `[ok]` on the invariants
  // above is vacuous because the interesting states were never explored.
  val wCeremonyDiverged = tuples(HONEST, HONEST).exists(q =>
    result.get(q._1) != result.get(q._2))
  val wCeremonyDone = HONEST.exists(k =>
    match result.get(k) { Done(_) => true | Failed(_) => false | Running => false })
  val wCeremonyBlamed = HONEST.exists(k =>
    match result.get(k) { Failed(bad) => bad != Set() | Done(_) => false | Running => false })

  // is_blame_response_complete (keygen_stages.rs): the response must contain a
  // share for exactly the set of parties that complained about `sender`.
  pure def blameResponseComplete(
    sender: Party,
    cs: Set[{ by: Party, about: Party }],
    rs: Set[{ by: Party, about: Party, ok: bool }]
  ): bool = {
    val expected = cs.filter(c => c.about == sender).map(c => c.by)
    val actual = rs.filter(r => r.by == sender).map(r => r.about)
    expected == actual
  }

  run blameResponseTest = all {
    // A response must contain a share for exactly the parties that complained.
    assert(blameResponseComplete(4,
      Set({ by: 1, about: 4 }, { by: 2, about: 4 }),
      Set({ by: 4, about: 1, ok: true }, { by: 4, about: 2, ok: true }))),
    // Missing one complainant's share -> incomplete.
    assert(not(blameResponseComplete(4,
      Set({ by: 1, about: 4 }, { by: 2, about: 4 }),
      Set({ by: 4, about: 1, ok: true })))),
    // Revealing a share nobody asked for -> incomplete.
    assert(not(blameResponseComplete(4,
      Set({ by: 1, about: 4 }),
      Set({ by: 4, about: 1, ok: true }, { by: 4, about: 3, ok: true })))),
  }

  // The key being generated. Under handover this is the NEW key's threshold,
  // not the current ceremony's party count.
  pure val KEY_THRESHOLD: int = thresholdOf(PARTIES.size())

  // validate_commitments (keygen_detail.rs): an honest party commits to exactly
  // `threshold + 1` coefficients. Declared `const` so harness.qnt can
  // instantiate a deliberately-broken variant as a permanent negative control,
  // rather than this being a manual edit-and-revert that nobody re-runs.
  const ENFORCE_COEFF_LENGTH: bool

  pure def commitmentLengthValid(len: int, keyThreshold: int): bool =
    len == keyThreshold + 1

  pure def commitmentAccepted(len: int): bool =
    not(ENFORCE_COEFF_LENGTH) or commitmentLengthValid(len, KEY_THRESHOLD)

  run coeffLengthTest = all {
    // threshold 2 -> exactly 3 coefficients
    assert(commitmentLengthValid(3, 2)),
    assert(not(commitmentLengthValid(4, 2))),   // over-long: raises the degree
    assert(not(commitmentLengthValid(2, 2))),   // too short: OOB on honest nodes
    assert(not(commitmentLengthValid(0, 2))),   // empty
  }
}
