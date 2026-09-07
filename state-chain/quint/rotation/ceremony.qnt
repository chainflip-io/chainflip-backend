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
      // unanimous has at most one key (a single voter cannot vote for two keys);
      // fold stands in for getOnlyElement, which Apalache cannot transpile.
      Success(unanimous.fold(0, (acc, k) => k))
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

// One ceremony driven to resolution by honest and Byzantine reports.
// Verified standalone; SeamSound discharges the oracle contract the chain
// and validator layers assume.
module ceremonyCheck {
  import types.* from "./types"
  import ceremony.* from "./ceremony"

  const CANDS: Set[Validator]
  const BYZ: Set[Validator]
  // StrongHonesty (DESIGN.md): every honest candidate reports the same outcome.
  // false = the keygen model's split (some honest Agreed, others Failed).
  const STRONG: bool
  // false = honest nodes report before KeygenResponseTimeout (the default
  // assumption). true = an outage: the timeout may fire with honest nodes
  // still pending, which is what makes the failure-threshold drop reachable.
  const HONEST_MAY_TIMEOUT: bool

  pure val HONEST: Set[Validator] = CANDS.exclude(BYZ)
  pure val FRESH_KEY: Key = 7
  pure val OTHER_KEY: Key = 8

  // What an honest node can report: the fresh key, or a failure blaming a
  // subset of the Byzantine candidates (empty = unattributed). Keygen model K1/K3.
  pure val HONEST_REPORTS: Set[Report] =
    Set(SuccessVote(FRESH_KEY))
      .union(BYZ.intersect(CANDS).powerset().map(b => FailureVote(b)))

  var rs: ResponseStatus
  var honestVerdict: Option[Report]
  var result: Option[Outcome]

  action init = all {
    rs' = newResponseStatus(CANDS),
    honestVerdict' = None,
    result' = None,
  }

  action honestReport(v: Validator, drawn: Report): bool =
    // Decode, don't constrain: under STRONG the first honest verdict binds the rest.
    val r = if (STRONG) (match honestVerdict { | Some(r0) => r0 | None => drawn }) else drawn
    all {
      result == None,
      HONEST.contains(v),
      canReport(rs, v),
      rs' = addReport(rs, v, r),
      honestVerdict' = (match honestVerdict { | Some(r0) => Some(r0) | None => Some(r) }),
      result' = None,
    }

  action byzReport(v: Validator, r: Report): bool = all {
    result == None,
    BYZ.contains(v),
    canReport(rs, v),
    rs' = addReport(rs, v, r),
    honestVerdict' = honestVerdict,
    result' = None,
  }

  // progress_rotation: resolve once every candidate has reported ...
  action resolveAllReported = all {
    result == None,
    rs.remaining == Set(),
    result' = Some(resolveKeygen(rs)),
    rs' = rs,
    honestVerdict' = honestVerdict,
  }

  // ... or when KeygenResponseTimeout elapses.
  action timeoutResolve = all {
    result == None,
    rs.remaining != Set(),
    HONEST_MAY_TIMEOUT or rs.remaining.intersect(HONEST) == Set(),
    result' = Some(resolveKeygen(rs)),
    rs' = rs,
    honestVerdict' = honestVerdict,
  }

  action step = {
    nondet v = CANDS.oneOf()
    nondet h = HONEST_REPORTS.oneOf()
    nondet k = Set(FRESH_KEY, OTHER_KEY).oneOf()
    nondet blamed = CANDS.powerset().oneOf()
    nondet asSuccess = Set(true, false).oneOf()
    val b = if (asSuccess) SuccessVote(k) else FailureVote(blamed)
    any {
      honestReport(v, h),
      byzReport(v, b),
      resolveAllReported,
      timeoutResolve,
    }
  }

  pure def isFailure(o: Outcome): bool =
    match o { | Failure(_) => true | Success(_) => false }
  pure def offendersOfOutcome(o: Outcome): Set[Validator] =
    match o { | Failure(off) => off | Success(_) => Set() }

  val C1_AcceptanceUnanimity = match result {
    | None => true
    | Some(o) => match o {
        | Success(k) => votersFor(rs, k) == CANDS
        | Failure(_) => true
      }
  }

  val C2_OffendersAreParticipants = match result {
    | None => true
    | Some(o) => offendersOfOutcome(o).subseteq(CANDS)
  }

  // Holds under STRONG with honest nodes reporting in time; fails under the
  // split (NC1, ceremonySplit) and under an outage (ceremonyOutage).
  val C3_HonestNeverOffender = match result {
    | None => true
    | Some(o) => offendersOfOutcome(o).intersect(HONEST) == Set()
  }

  // Every resolved outcome is one the oracle may return.
  val SeamSound = match result {
    | None => true
    | Some(o) => oracleOutcomes(CANDS, FRESH_KEY, BYZ, STRONG and not(HONEST_MAY_TIMEOUT)).contains(o)
  }

  // NC2 target: offenders were non-empty before the drop and empty after it.
  // Must FAIL on ceremonyOutage.
  val NC2_DropNeverEmpties_MustFailHere = match result {
    | None => true
    | Some(o) => not(o == Failure(Set()) and offendersOfOutcome(resolve(rs)) != Set())
  }

  val wResolvedSuccess = result == Some(Success(FRESH_KEY))
  val wResolvedFailure = match result { | None => false | Some(o) => isFailure(o) }
  val wByzantinePunished = match result {
    | None => false
    | Some(o) => offendersOfOutcome(o).intersect(BYZ) != Set()
  }
  val wDropped = match result {
    | None => false
    | Some(o) => o == Failure(Set()) and offendersOfOutcome(resolve(rs)) != Set()
  }
}
