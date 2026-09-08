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

module main {
  import types.* from "./types"
  import chain.* from "./chain"
  import rotation(
    VALIDATORS = Set(1, 2, 3, 4),
    BYZ = Set(4),
    CHAINS = Map("btc" -> { kind: Utxo, vault: Active }, "evm" -> { kind: NonUtxo, vault: Active }),
    MIN_SIZE = 2,
    MAX_SIZE = 4,
    CONTRACTION_PERCENT = 30,
    STRONG = true,
    FAIR = false,
    K_BLOCKS = 40
  ).* from "./validator"

  pure def inKeygen(p: Phase): bool = match p { | KeygensInProgress(_) => true | _ => false }
  pure def inHandover(p: Phase): bool = match p { | KeyHandoversInProgress(_) => true | _ => false }
  pure def inActivating(p: Phase): bool = match p { | ActivatingKeys(_) => true | _ => false }
  pure def inSessionRotating(p: Phase): bool = match p { | SessionRotating(_) => true | _ => false }

  // One full rotation with a BTC handover, every delivery in the happy order.
  // Deterministic (no nondet), so it proves reachability without sampling.
  run happyPathTest =
    init
      .then(epochElapses)
      .then(block(Set(), Set(), true))
      .expect(inKeygen(w.phase))
      .then(keygenOutcome("btc", true, Set()))
      .then(keygenOutcome("evm", true, Set()))
      .then(verificationResult("btc", true, Set()))
      .then(verificationResult("evm", true, Set()))
      .then(block(Set(), Set(), true))
      .expect(inHandover(w.phase))
      .expect(w.chains.get("evm").status == KeyHandoverComplete(2))
      .then(handoverOutcome("btc", true, Set()))
      .then(verificationResult("btc", true, Set()))
      .then(block(Set(), Set(), true))
      .expect(inActivating(w.phase))
      .then(signatureReadyOn("btc"))
      .then(signatureReadyOn("evm"))
      .then(block(Set(), Set(), true))
      .expect(inSessionRotating(w.phase))
      .then(block(Set(), Set(), true))
      .expect(all {
        w.phase == Idle,
        w.epoch == 2,
        w.authorities == Set(1, 2, 3, 4),
        w.panics == Set(),
        w.logErrors == Set(),
        CHAIN_IDS.forall(id => isComplete(w.chains.get(id))),
        w.chains.get("btc").activeKey == Some({ epoch: 2, key: 2 }),
        w.chains.get("btc").handoverRan,
      })
}

// Adds an uninitialised chain: activation completes on it without a key
// (ChainNotInitialized), so R6 must tolerate a chain with no key.
module uninit {
  import types.* from "./types"
  import chain.* from "./chain"
  import rotation(
    VALIDATORS = Set(1, 2, 3, 4),
    BYZ = Set(4),
    CHAINS = Map(
      "btc" -> { kind: Utxo, vault: Active },
      "evm" -> { kind: NonUtxo, vault: Active },
      "sol" -> { kind: NonUtxo, vault: Uninitialised }),
    MIN_SIZE = 2,
    MAX_SIZE = 4,
    CONTRACTION_PERCENT = 30,
    STRONG = true,
    FAIR = false,
    K_BLOCKS = 40
  ).* from "./validator"

  val W9_UninitialisedChainCompletesWithoutKey =
    w.hist.epochsAdvanced >= 1 and w.chains.get("sol").activeKey == None and isComplete(w.chains.get("sol"))
}

// The keygen model's split: honest nodes can be attributed. Safety must
// still hold; the price is honest nodes banned (W8) and NC1 at the ceremony layer.
module split {
  import types.* from "./types"
  import chain.* from "./chain"
  import rotation(
    VALIDATORS = Set(1, 2, 3, 4),
    BYZ = Set(4),
    CHAINS = Map("btc" -> { kind: Utxo, vault: Active }, "evm" -> { kind: NonUtxo, vault: Active }),
    MIN_SIZE = 2,
    MAX_SIZE = 4,
    CONTRACTION_PERCENT = 30,
    STRONG = false,
    FAIR = false,
    K_BLOCKS = 40
  ).* from "./validator"

  val W8_HonestBannedUnderSplit = match rsOf(w.phase) {
    | Some(rs) => rs.banned.intersect(HONEST) != Set()
    | None => false
  }
}

// Fair scheduler: environment deliveries precede blocks, honest ceremonies
// succeed, Byzantine failures are attributed, signing and activation succeed,
// no safe-mode/broadcast/bidder churn. Liveness properties are bounded
// invariants here; K_BLOCKS is the measured longest rotation plus margin.
module fair {
  import types.* from "./types"
  import chain.* from "./chain"
  import rotation(
    VALIDATORS = Set(1, 2, 3, 4),
    BYZ = Set(4),
    CHAINS = Map("btc" -> { kind: Utxo, vault: Active }, "evm" -> { kind: NonUtxo, vault: Active }),
    MIN_SIZE = 2,
    MAX_SIZE = 4,
    CONTRACTION_PERCENT = 30,
    STRONG = true,
    FAIR = true,
    K_BLOCKS = 14
  ).* from "./validator"

  // The longest fair rotation: Byzantine fails keygen (attributed), one
  // restart with it banned, then a clean run. Nine blocks.
  run fairRotationCompletesTest =
    init
      .then(epochElapses)
      .then(block(Set(), Set(), true))
      .then(keygenOutcome("btc", false, Set(4)))
      .then(keygenOutcome("evm", true, Set()))
      .then(verificationResult("evm", true, Set()))
      .then(block(Set(), Set(), true))
      .expect(w.hist.keygenRestarts == 1)
      .then(keygenOutcome("btc", true, Set()))
      .then(keygenOutcome("evm", true, Set()))
      .then(verificationResult("btc", true, Set()))
      .then(verificationResult("evm", true, Set()))
      .then(block(Set(), Set(), true))
      .then(handoverOutcome("btc", true, Set()))
      .then(verificationResult("btc", true, Set()))
      .then(block(Set(), Set(), true))
      .then(signatureReadyOn("btc"))
      .then(signatureReadyOn("evm"))
      .then(block(Set(), Set(), true))
      .then(block(Set(), Set(), true))
      .expect(all {
        w.phase == Idle,
        w.epoch == 2,
        w.authorities == Set(1, 2, 3),
        w.blocks - w.hist.rotationStartedAt <= K_BLOCKS,
        w.hist.aborts == 0,
      })
}

// One UTXO chain only: the smallest instance that still exercises keygen,
// handover, activation and the session steps. Used for exhaustive checks
// when the two-chain instance does not fit. H4_ConsSoundness is trivial
// here (a one-chain consensus status is the chain's own), so it must never
// be reported as verified from this instance.
module main1 {
  import types.* from "./types"
  import chain.* from "./chain"
  import rotation(
    VALIDATORS = Set(1, 2, 3, 4),
    BYZ = Set(4),
    CHAINS = Map("btc" -> { kind: Utxo, vault: Active }),
    MIN_SIZE = 2,
    MAX_SIZE = 4,
    CONTRACTION_PERCENT = 30,
    STRONG = true,
    FAIR = false,
    K_BLOCKS = 40
  ).* from "./validator"
}
