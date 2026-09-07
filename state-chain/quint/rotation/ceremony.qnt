// On-chain resolution of one keygen or key-handover ceremony.
//
// Mirrors cf-threshold-signature/src/response_status.rs (ResponseStatus,
// resolve_keygen_outcome) and the failure-threshold drop in lib.rs
// progress_rotation. Verified standalone (module ceremonyCheck); the chain
// and validator layers consume outcomes through types.oracleOutcomes.
module ceremony {
  import types.* from "./types"

  // report_keygen_outcome / report_key_handover_outcome payload.
  type Report = SuccessVote(Key) | FailureVote(Set[Validator])

  type ResponseStatus = {
    candidates: Set[Validator],
    remaining: Set[Validator],
    successVotes: Set[{ voter: Validator, key: Key }],
    failureVoters: Set[Validator],
    blames: Set[{ voter: Validator, blamed: Validator }],
  }

  pure def newResponseStatus(cands: Set[Validator]): ResponseStatus = {
    candidates: cands,
    remaining: cands,
    successVotes: Set(),
    failureVoters: Set(),
    blames: Set(),
  }

  // handle_key_ceremony_report!: only a remaining candidate may report, once.
  pure def canReport(rs: ResponseStatus, v: Validator): bool = rs.remaining.contains(v)

  // add_success_vote / add_failure_vote. Blames outside the candidate set are
  // dropped (handle_key_ceremony_report! partitions them off with a warning).
  pure def addReport(rs: ResponseStatus, v: Validator, r: Report): ResponseStatus =
    match r {
    | SuccessVote(k) => { ...rs,
        remaining: rs.remaining.exclude(Set(v)),
        successVotes: rs.successVotes.union(Set({ voter: v, key: k })) }
    | FailureVote(blamed) => { ...rs,
        remaining: rs.remaining.exclude(Set(v)),
        failureVoters: rs.failureVoters.union(Set(v)),
        blames: rs.blames.union(
          blamed.intersect(rs.candidates).map(b => { voter: v, blamed: b })) }
    }

  pure def votersFor(rs: ResponseStatus, k: Key): Set[Validator] =
    rs.successVotes.filter(s => s.key == k).map(s => s.voter)

  pure def votedKeys(rs: ResponseStatus): Set[Key] = rs.successVotes.map(s => s.key)

  pure def blameCount(rs: ResponseStatus, b: Validator): int =
    rs.blames.filter(x => x.blamed == b).size()

  pure def superMajority(rs: ResponseStatus): int = successThreshold(rs.candidates.size())

  // resolve_keygen_outcome, without final_key_check (see resolveHandover).
  //   all candidates on one key            -> Success
  //   some key at the super-majority        -> punish other success voters + failure voters
  //   else failure voters at super-majority -> punish all success voters
  //   else                                  -> punish no voter
  //   plus: heavily blamed (>= super-majority blames) and non-responders.
  pure def resolve(rs: ResponseStatus): Outcome =
    val n = rs.candidates.size()
    val sm = superMajority(rs)
    val unanimous = votedKeys(rs).filter(k => votersFor(rs, k).size() == n)
    if (unanimous != Set())
      Success(unanimous.getOnlyElement())
    else
      // At most one key can reach the super-majority (t + 1 > n / 2).
      val exemptKeys = votedKeys(rs).filter(k => votersFor(rs, k).size() >= sm)
      val punishedVoters =
        if (exemptKeys != Set())
          rs.successVotes.filter(s => not(exemptKeys.contains(s.key))).map(s => s.voter)
            .union(rs.failureVoters)
        else if (rs.failureVoters.size() >= sm)
          rs.successVotes.map(s => s.voter)
        else
          Set()
      val heavilyBlamed = rs.candidates.filter(b => blameCount(rs, b) >= sm)
      Failure(punishedVoters.union(heavilyBlamed).union(rs.remaining))

  // progress_rotation: an offender set at or above the failure threshold is
  // dropped to empty ("we don't want to risk liveness"), so nobody is banned.
  pure def dropAtFailureThreshold(o: Outcome, n: int): Outcome =
    match o {
    | Failure(off) => if (off.size() < failureThreshold(n)) Failure(off) else Failure(Set())
    | Success(k) => Success(k)
    }

  pure def resolveKeygen(rs: ResponseStatus): Outcome =
    dropAtFailureThreshold(resolve(rs), rs.candidates.size())

  // Handover adds final_key_check: handover_key_matches(current, reported)
  // must hold, else the outcome is Err(Default::default()) = Failure(Set()).
  pure def resolveHandover(rs: ResponseStatus, expectedKey: Key): Outcome =
    match resolve(rs) {
    | Success(k) => if (k == expectedKey) Success(k) else Failure(Set())
    | Failure(off) => dropAtFailureThreshold(Failure(off), rs.candidates.size())
    }

  // --- tests (response_status.rs tests, progress_rotation behaviour) ---

  pure val C4: Set[Validator] = Set(1, 2, 3, 4)

  pure def reportAll(rs: ResponseStatus, voters: Set[Validator], r: Report): ResponseStatus =
    voters.fold(rs, (acc, v) => addReport(acc, v, r))

  run unanimitySucceedsTest = all {
    assert(resolveKeygen(reportAll(newResponseStatus(C4), C4, SuccessVote(7))) == Success(7)),
  }

  run dissenterIsPunishedTest =
    val rs = addReport(reportAll(newResponseStatus(C4), Set(1, 2, 3), SuccessVote(7)), 4, SuccessVote(8))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run failureVoterAgainstSuperMajorityIsPunishedTest =
    val rs = addReport(reportAll(newResponseStatus(C4), Set(1, 2, 3), SuccessVote(7)), 4, FailureVote(Set()))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run successVoterAgainstFailureSuperMajorityIsPunishedTest =
    val rs = addReport(reportAll(newResponseStatus(C4), Set(1, 2, 3), FailureVote(Set())), 4, SuccessVote(7))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run noConsensusPunishesNoVoterTest =
    // 2 vs 2: neither side reaches the super-majority of 3.
    val rs = reportAll(reportAll(newResponseStatus(C4), Set(1, 2), SuccessVote(7)), Set(3, 4), FailureVote(Set()))
    all { assert(resolveKeygen(rs) == Failure(Set())) }

  run heavilyBlamedIsPunishedTest =
    // 4 is blamed three times (>= 3) and is also a non-responder.
    val rs = reportAll(newResponseStatus(C4), Set(1, 2, 3), FailureVote(Set(4)))
    all {
      assert(resolve(rs) == Failure(Set(4))),
      assert(resolveKeygen(rs) == Failure(Set(4))),
    }

  run blamesOutsideCandidatesAreDroppedTest =
    val rs = reportAll(newResponseStatus(C4), Set(1, 2, 3), FailureVote(Set(9)))
    all { assert(rs.blames == Set()) }

  run failureThresholdDropTest =
    // Two non-responders at n=4: |off| = 2 >= failure threshold 2 -> dropped.
    val rs = reportAll(newResponseStatus(C4), Set(1, 2), SuccessVote(7))
    all {
      assert(resolve(rs) == Failure(Set(3, 4))),
      assert(resolveKeygen(rs) == Failure(Set())),
    }

  run singleNonResponderIsKeptTest =
    val rs = reportAll(newResponseStatus(C4), Set(1, 2, 3), SuccessVote(7))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run handoverKeyMismatchIsUnattributedTest = all {
    assert(resolveHandover(reportAll(newResponseStatus(C4), C4, SuccessVote(7)), 7) == Success(7)),
    assert(resolveHandover(reportAll(newResponseStatus(C4), C4, SuccessVote(8)), 7) == Failure(Set())),
  }
}
