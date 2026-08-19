module seam {
  import types.* from "./types"
  import broadcast.* from "./broadcast"
  import oracle.* from "./oracle"

  pure def toOracle(o: Outcome): OracleResult =
    match o {
    | Agreed(m) => OkAgreed(m)
    | Attributed(bad) => if (bad == Set()) OkFailed else OkAttributed(bad)
    | Unattributed => OkFailed
    }

  // Every outcome the concrete verify() can produce at an honest party must
  // satisfy the contract keygen.qnt assumes. `out` is broadcast.qnt's state
  // variable, already computed from the corrected `verify(k, present, claims)`.
  val SeamSound = HONEST.forall(k => oracleWellFormed(toOracle(out.get(k)), HONEST))
}
