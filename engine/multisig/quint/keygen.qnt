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

  pure val ALL_PAIRS: Set[{ from: Party, to: Party }] =
    tuples(PARTIES, PARTIES).map(t => { from: t._1, to: t._2 })

  action init = all {
    stage' = PARTIES.mapBy(_ => PubkeyShares0),
    result' = PARTIES.mapBy(_ => Running),
    shareValid' = ALL_PAIRS,
    complaints' = Set(),
    revealed' = Set(),
  }

  // Two distinct candidate keys, so K2 (no two honest parties finish with
  // different keys) is not vacuously true.
  pure val KEY_A: Party -> Vote = PARTIES.mapBy(i => Sent(i))
  pure val KEY_B: Party -> Vote = PARTIES.mapBy(i => Sent(i + 1))

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
      complaints.filter(c => HONEST.contains(c.about))
        .map(c => { by: c.about, about: c.by,
                    ok: shareValid.contains({ from: c.about, to: c.by }) })

    all {
      stage' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) stage.get(k)
        else if (isVerifyStage(stage.get(k)) and
                 (match res.get(k) {
                  | OkAgreed(_) => false
                  | OkAttributed(_) => true
                  | OkFailed => true
                 })) stage.get(k)
        else nextStage(stage.get(k))),
      result' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) result.get(k)
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
}
