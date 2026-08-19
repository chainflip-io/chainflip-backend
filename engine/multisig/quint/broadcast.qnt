module broadcast {
  import types.* from "./types"

  // find_frequent_element (client/utils.rs:26): the value strictly more than
  // `t` of the reports agree on, if any. At most one value can qualify.
  pure def frequent(reports: List[Vote], t: int): Opt = {
    val distinct = reports.foldl(Set(), (acc, v) => acc.union(Set(v)))
    val winners = distinct.filter(v => reports.select(w => w == v).length() > t)
    if (winners.size() == 0) NoQuorum else Found(winners.fold(Nothing, (_, v) => v))
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
}
