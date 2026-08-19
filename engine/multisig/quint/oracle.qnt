module oracle {
  import types.* from "./types"

  type OracleResult =
    | OkAgreed(Party -> Vote)
    | OkAttributed(Set[Party])
    | OkFailed

  // The postconditions broadcast.qnt discharges (L1-L6). keygen.qnt consumes
  // the oracle as one atomic step; this predicate is the seam made explicit.
  //
  //   OkAgreed(m)     - m is identical at every honest party, and records what
  //                     each honest sender actually sent          (L2, L3)
  //   OkAttributed(b) - b is non-empty and contains no honest party    (L1)
  //   OkFailed        - the stage failed and nobody is blamed
  //
  // Honest parties may diverge between OkAgreed and a failure result, but never
  // between two different OkAgreed maps: full outcome agreement is false.
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
