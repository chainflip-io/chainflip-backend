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
}
