module oracle {
  import types.* from "./types"

  type OracleResult =
    | OkAgreed(Party -> Vote)
    | OkAttributed(Set[Party])
    | OkFailed

  // What this predicate mechanically checks, arm by arm:
  //   OkAgreed(_)     - unconditional (true)
  //   OkAttributed(b) - `b != Set()` is dead: toOracle already maps an empty
  //                     blame set to OkFailed. The live clause is
  //                     `b.intersect(honest) == Set()`, i.e. L1.
  //   OkFailed        - unconditional (true)
  // This is per-party, so agreement (L2) is NOT checked here - see
  // SeamAgreementSound in seam.qnt.
  pure def oracleWellFormed(r: OracleResult, honest: Set[Party]): bool =
    match r {
    | OkAgreed(_) => true
    | OkAttributed(bad) => bad != Set() and bad.intersect(honest) == Set()
    | OkFailed => true
    }

  run oracleContractTest = all {
    // Blaming an honest party violates the contract (this is L1).
    assert(not(oracleWellFormed(OkAttributed(Set(1)), Set(1, 2, 3)))),
    // Blaming only Byzantine parties satisfies it.
    assert(oracleWellFormed(OkAttributed(Set(4)), Set(1, 2, 3))),
    // An empty blame set is not attribution; it must be OkFailed instead.
    assert(not(oracleWellFormed(OkAttributed(Set()), Set(1, 2, 3)))),
    // Failing without blame is always permitted.
    assert(oracleWellFormed(OkFailed, Set(1, 2, 3))),
  }
}
