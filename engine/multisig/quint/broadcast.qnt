module broadcast {
  import types.* from "./types"

  // find_frequent_element (client/utils.rs:26): the value strictly more than
  // `t` of the reports agree on, if any. At most one value can qualify.
  pure def frequent(reports: List[Vote], t: int): Opt = {
    val distinct = reports.foldl(Set(), (acc, v) => acc.union(Set(v)))
    val winners = distinct.filter(v => reports.select(w => w == v).length() > t)
    if (winners.size() == 0) NoQuorum else Found(winners.fold(Nothing, (_, v) => v))
  }

  // PubkeySharesStage0 (keygen_stages.rs:129). One round, threshold taken over
  // the SHARING participants only, and it can never attribute: all its failure
  // paths report an empty set.
  pure def quorumVote(k: Party, sharers: Set[Party], ss: Set[Send]): Opt = {
    val t = thresholdOf(sharers.size())
    val received = sharers.fold(List(), (acc, j) => {
      val m = ss.filter(s => s.from == j and s.to == k)
      if (m.size() == 1) acc.append(m.fold(Nothing, (_, x) => x.vote)) else acc
    })
    if (received.length() <= t) NoQuorum else frequent(received, t)
  }

  run frequentTest = all {
    // 3 of 4 agree, t = 2 -> quorum
    assert(frequent([Sent(1), Sent(1), Sent(1), Nothing], 2) == Found(Sent(1))),
    // split 2-2, t = 2 -> no quorum
    assert(frequent([Sent(1), Sent(0), Sent(1), Nothing], 2) == NoQuorum),
    // 3 of 4 agree the party sent nothing -> attributable
    assert(frequent([Nothing, Nothing, Nothing, Sent(1)], 2) == Found(Nothing)),
    // exactly t occurrences is NOT a quorum (strict inequality)
    assert(frequent([Sent(1), Sent(1), Sent(0), Sent(0)], 2) == NoQuorum),
  }

  // What k heard `by` claim about `about`. Absence of a claim reads as Nothing,
  // matching BroadcastStage::finalize inserting None for unreceived messages.
  pure def claimed(cs: Set[Claim], by: Party, k: Party, about: Party): Vote = {
    val c = cs.filter(x => x.by == by and x.to == k and x.about == about)
    if (c.size() == 1) c.fold(Nothing, (_, x) => x.vote) else Nothing
  }

  // A round-2 message carries an entry for every party (the Rust rejects any
  // whose key set is not exactly the participant set), so presence is tracked
  // separately from content: a message reporting Nothing about everyone is
  // present, and is not the same as no message at all.
  pure def sentRound2(ps: Set[(Party, Party)], by: Party, k: Party): bool =
    ps.contains((by, k))

  // verify_broadcasts (broadcast_verification.rs) as executed by party k.
  pure def verify(k: Party, ps: Set[(Party, Party)], cs: Set[Claim]): Outcome = {
    val present = PARTIES.filter(j => sentRound2(ps, j, k))
    if (present.size() <= T) Unattributed
    else {
      val resolved = PARTIES.mapBy(i =>
        frequent(present.fold(List(), (acc, j) => acc.append(claimed(cs, j, k, i))), T))
      val blamed = PARTIES.filter(i => resolved.get(i) == Found(Nothing))
      val stuck = PARTIES.filter(i => resolved.get(i) == NoQuorum)
      // Order matters: attributable blame wins, and an unresolved value must
      // never become blame. Claims of non-receipt are unfalsifiable, so a
      // colluding minority denying quorum could otherwise frame honest nodes.
      if (blamed.size() > 0) Attributed(blamed)
      else if (stuck.size() > 0) Unattributed
      else Agreed(PARTIES.mapBy(i => match resolved.get(i) {
        | Found(v) => v
        | NoQuorum => Nothing
      }))
    }
  }

  // Helper for tests: everyone honestly echoes that `about` sent `v`.
  pure def unanimous(about: Party, v: Vote): Set[Claim] =
    tuples(PARTIES, PARTIES).map(t => { by: t._1, to: t._2, about: about, vote: v })

  pure def allClaims(f: (Party) => Vote): Set[Claim] =
    tuples(PARTIES, PARTIES, PARTIES).map(t =>
      { by: t._1, to: t._2, about: t._3, vote: f(t._3) })

  pure val ALL_PRESENT: Set[(Party, Party)] = tuples(PARTIES, PARTIES)

  run verifyTest = all {
    // Everyone agrees every party sent its index as a value -> Agreed.
    assert(match verify(1, ALL_PRESENT, allClaims(i => Sent(i))) {
      | Agreed(m) => m.get(3) == Sent(3)
      | Attributed(_) => false
      | Unattributed => false
    }),
    // Everyone agrees party 4 sent nothing -> attributable to 4, and only 4.
    assert(match verify(1, ALL_PRESENT, allClaims(i => if (i == 4) Nothing else Sent(i))) {
      | Attributed(bad) => bad == Set(4)
      | Agreed(_) => false
      | Unattributed => false
    }),
    // Too few round-2 messages -> Unattributed, nobody blamed.
    assert(verify(1, Set(), Set()) == Unattributed),
  }

  // Party `about` is reported inconsistently: parties 1,2 say one thing and
  // 3,4 another, so with T=2 nothing exceeds the quorum for that party.
  pure def claimsSplitAbout(about: Party): Set[Claim] =
    tuples(PARTIES, PARTIES, PARTIES).map(t =>
      { by: t._1, to: t._2, about: t._3,
        vote: if (t._3 == about) (if (t._1 <= 2) Sent(0) else Sent(1))
              else Sent(t._3) })

  // Party 4 unanimously reported as having sent nothing (attributable), while
  // party 3 is split with no quorum either way (unresolved, unattributable).
  pure val claimsBlamedAndStuck: Set[Claim] =
    tuples(PARTIES, PARTIES, PARTIES).map(t =>
      { by: t._1, to: t._2, about: t._3,
        vote: if (t._3 == 4) Nothing
              else if (t._3 == 3) (if (t._1 <= 2) Sent(0) else Sent(1))
              else Sent(t._3) })

  run stuckBranchTest = all {
    // No quorum on party 3 -> stage fails, and NOBODY is blamed. Blaming here
    // would let a colluding minority frame an honest node by denying quorum.
    assert(verify(1, ALL_PRESENT, claimsSplitAbout(3)) == Unattributed),
    // Blame wins over unresolved, and the unresolved party is NOT added to it.
    assert(match verify(1, ALL_PRESENT, claimsBlamedAndStuck) {
      | Attributed(bad) => bad == Set(4)
      | Agreed(_) => false
      | Unattributed => false
    }),
  }

  // Slots an adversary may act on. Choices are encoded as characteristic-
  // function subsets of these, so EVERY draw is well-formed by construction.
  // Drawing arbitrary subsets and rejecting the ill-formed ones with a guard
  // makes the simulator reject ~every draw (a valid claim set has probability
  // about 2^-16), leaving `step` unreachable and every run vacuously ok.
  pure val SEND_SLOTS: Set[(Party, Party)] = tuples(BYZ, PARTIES)
  pure val CLAIM_SLOTS: Set[(Party, Party, Party)] = tuples(BYZ, PARTIES, PARTIES)

  action init = all {
    sends' = Set(),
    claims' = Set(),
    present' = Set(),
    out' = PARTIES.mapBy(i => Unattributed),
  }

  action step = {
    // One bit per honest party: which of the two values it broadcasts.
    nondet hvHigh = HONEST.powerset().oneOf()
    // Byzantine round 1: sent-at-all, and which value.
    nondet bsMade = SEND_SLOTS.powerset().oneOf()
    nondet bsHigh = SEND_SLOTS.powerset().oneOf()
    // Byzantine round 2: which messages exist, then per-slot content.
    nondet bcPresent = SEND_SLOTS.powerset().oneOf()
    nondet bcMade = CLAIM_SLOTS.powerset().oneOf()
    nondet bcHigh = CLAIM_SLOTS.powerset().oneOf()

    val honestSends = tuples(HONEST, PARTIES).map(t =>
      { from: t._1, to: t._2, vote: Sent(if (hvHigh.contains(t._1)) 1 else 0) })
    val byzSends = SEND_SLOTS.filter(sl => bsMade.contains(sl)).map(sl =>
      { from: sl._1, to: sl._2, vote: Sent(if (bsHigh.contains(sl)) 1 else 0) })
    val allSends = honestSends.union(byzSends)

    // An honest party echoes to everyone exactly what it received.
    val honestClaims = tuples(HONEST, PARTIES, PARTIES).map(t => {
      val heard = allSends.filter(s => s.from == t._3 and s.to == t._1)
      { by: t._1, to: t._2, about: t._3,
        vote: if (heard.size() == 1) heard.fold(Nothing, (_, x) => x.vote) else Nothing }
    })
    // A slot the Byzantine party omits reads as Nothing (it claims it heard
    // nothing from that party) - the message still exists if bcPresent says so.
    val byzClaims = CLAIM_SLOTS.filter(sl => bcMade.contains(sl)).map(sl =>
      { by: sl._1, to: sl._2, about: sl._3,
        vote: if (bcHigh.contains(sl)) Sent(1) else Sent(0) })
    val allClaims = honestClaims.union(byzClaims)

    // Honest parties always send their round-2 message to everyone.
    val allPresent = tuples(HONEST, PARTIES).union(bcPresent)

    all {
      sends' = allSends,
      claims' = allClaims,
      present' = allPresent,
      out' = PARTIES.mapBy(k => verify(k, allPresent, allClaims)),
    }
  }

  var sends: Set[Send]
  var claims: Set[Claim]
  var present: Set[(Party, Party)]
  var out: Party -> Outcome

  // L1: an honest party never blames an honest party. THE headline property.
  val L1_NoFalseBlame = HONEST.forall(k =>
    match out.get(k) {
    | Attributed(bad) => bad.intersect(HONEST) == Set()
    | Agreed(_) => true
    | Unattributed => true
    })

  // L2: two honest parties that both agree, agree on the same map.
  val L2_ValueAgreement = tuples(HONEST, HONEST).forall(p =>
    match out.get(p._1) {
    | Agreed(m1) => match out.get(p._2) {
        | Agreed(m2) => m1 == m2
        | Attributed(_) => true
        | Unattributed => true
      }
    | Attributed(_) => true
    | Unattributed => true
    })

  // L3: an agreed map records what honest senders actually sent.
  val L3_HonestValuePreservation = HONEST.forall(k =>
    match out.get(k) {
    | Agreed(m) => HONEST.forall(i =>
        sends.exists(s => s.from == i and s.to == k and s.vote == m.get(i)))
    | Attributed(_) => true
    | Unattributed => true
    })

  // L4: divergence is permitted between Agreed and failure, but never between
  // two different Agreed maps, and never into blame. Full outcome agreement is
  // FALSE for this protocol - see the spec for the counterexample.
  val L4_SafeDivergence = L1_NoFalseBlame and L2_ValueAgreement

  // Witnesses. These are not properties to hold - they are coverage checks.
  // Each must be reached in > 0 traces, or the run's `[ok]` on the invariants
  // above is vacuous because the interesting states were never explored.
  val wAgreed = HONEST.exists(k =>
    match out.get(k) { Agreed(_) => true | Attributed(_) => false | Unattributed => false })
  val wAttributed = HONEST.exists(k =>
    match out.get(k) { Attributed(_) => true | Agreed(_) => false | Unattributed => false })
  val wUnattributed = HONEST.exists(k =>
    match out.get(k) { Unattributed => true | Agreed(_) => false | Attributed(_) => false })
  // The verified L4 divergence: two honest parties reaching different outcomes.
  val wDiverged = tuples(HONEST, HONEST).exists(p => out.get(p._1) != out.get(p._2))

  // L5: with no Byzantine deviation (everyone sends one consistent value and
  // echoes truthfully), every party reaches Agreed and recovers what was sent.
  run L5_LivenessTest = {
    val ss = tuples(PARTIES, PARTIES).map(t => { from: t._1, to: t._2, vote: Sent(t._1) })
    val cs = tuples(PARTIES, PARTIES, PARTIES).map(t =>
      { by: t._1, to: t._2, about: t._3, vote: Sent(t._3) })
    assert(PARTIES.forall(k => match verify(k, ALL_PRESENT, cs) {
      | Agreed(m) => PARTIES.forall(i => m.get(i) == Sent(i))
      | Attributed(_) => false
      | Unattributed => false
    }))
  }

  // L6: honest parties never disagree on a quorum-vote result, and the
  // primitive never blames anyone (it returns no blame set at all).
  val L6_VoteAgreement = tuples(HONEST, HONEST).forall(p =>
    quorumVote(p._1, PARTIES, sends) == quorumVote(p._2, PARTIES, sends)
    or quorumVote(p._1, PARTIES, sends) == NoQuorum
    or quorumVote(p._2, PARTIES, sends) == NoQuorum)

  run quorumVoteTest = all {
    // 3 sharers all sending the same value: threshold over 3 is 1, so 3 > 1.
    assert(quorumVote(1, Set(1, 2, 3),
      Set({ from: 1, to: 1, vote: Sent(7) },
          { from: 2, to: 1, vote: Sent(7) },
          { from: 3, to: 1, vote: Sent(7) })) == Found(Sent(7))),
    // A 2-1 split over 3 sharers: 2 > 1, so the majority still carries.
    assert(quorumVote(1, Set(1, 2, 3),
      Set({ from: 1, to: 1, vote: Sent(7) },
          { from: 2, to: 1, vote: Sent(7) },
          { from: 3, to: 1, vote: Sent(9) })) == Found(Sent(7))),
    // A 1-1-1 split reaches nothing.
    assert(quorumVote(1, Set(1, 2, 3),
      Set({ from: 1, to: 1, vote: Sent(7) },
          { from: 2, to: 1, vote: Sent(8) },
          { from: 3, to: 1, vote: Sent(9) })) == NoQuorum),
  }
}
