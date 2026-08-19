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

  // SeamSound is per-party, so it cannot express agreement: after `toOracle`
  // folds an empty blame set into OkFailed, its only falsifiable content is
  // L1 (no honest party is blamed). The ceremony model also assumes L2 - it
  // picks ONE agreed key per step - so the seam has to cover that separately,
  // and a relational invariant is the only way to state it.
  val SeamAgreementSound = tuples(HONEST, HONEST).forall(p =>
    match toOracle(out.get(p._1)) {
    | OkAgreed(m1) => match toOracle(out.get(p._2)) {
        | OkAgreed(m2) => m1 == m2
        | OkAttributed(_) => true
        | OkFailed => true
      }
    | OkAttributed(_) => true
    | OkFailed => true
    })
}
