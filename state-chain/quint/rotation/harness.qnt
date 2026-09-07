// Instantiations with concrete `const` values. Every check targets one of
// these modules via --main; the parameterised modules cannot run directly.

module ceremonyStrong {
  import ceremonyCheck(
    CANDS = Set(1, 2, 3, 4), BYZ = Set(4), STRONG = true, HONEST_MAY_TIMEOUT = false
  ).* from "./ceremony"
}

// The keygen model's split outcome: honest nodes may disagree. NC1 must fail here.
module ceremonySplit {
  import ceremonyCheck(
    CANDS = Set(1, 2, 3, 4), BYZ = Set(4), STRONG = false, HONEST_MAY_TIMEOUT = false
  ).* from "./ceremony"

  val NC1_HonestNeverOffender_MustFailHere = C3_HonestNeverOffender
}

// An outage: honest nodes may still be pending when the timeout fires.
// The failure-threshold drop is only reachable here at n=4/f=1. NC2 must fail.
module ceremonyOutage {
  import ceremonyCheck(
    CANDS = Set(1, 2, 3, 4), BYZ = Set(4), STRONG = true, HONEST_MAY_TIMEOUT = true
  ).* from "./ceremony"
}
