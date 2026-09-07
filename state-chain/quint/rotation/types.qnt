// Shared vocabulary for the authority rotation model.
//
// Mirrors: cf-primitives (ids), cf-chains ChainCrypto::key_handover_is_required
// (UTXO chains need a handover), utilities/src/lib.rs (thresholds).
module types {
  type Validator = int
  type Chain = str
  type Key = int
  type Epoch = int

  type Option[a] = Some(a) | None

  // key_handover_is_required() == UtxoChain::get(): only UTXO chains hand over.
  type ChainKind = Utxo | NonUtxo
  // cf-vaults: VaultStartBlockNumbers non-empty => Active; ChainInitialized
  // unset => Uninitialised (activation completes without setting a key).
  type Vault = Active | Uninitialised
  type ChainSpec = { kind: ChainKind, vault: Vault }

  // A ceremony as the chain and validator layers see it (the oracle).
  type Outcome = Success(Key) | Failure(Set[Validator])

  // utilities/src/lib.rs: threshold_from_share_count and friends.
  pure def thresholdOf(n: int): int = if (n == 0) 0 else (2 * n - 1) / 3
  pure def successThreshold(n: int): int = thresholdOf(n) + 1
  pure def failureThreshold(n: int): int = n - thresholdOf(n)

  // sp_arithmetic Percent * u32: Rounding::NearestPrefDown (ties round down).
  pure def percentOf(percent: int, n: int): int =
    val q = (percent * n) / 100
    val r = (percent * n) % 100
    if (r > 50) q + 1 else q

  // cf-validator start_keygen_attempt:
  // (Percent::one() - MaxAuthoritySetContractionPercentage) * current_authority_count
  pure def contractionFloor(current: int, contractionPercent: int): int =
    percentOf(100 - contractionPercent, current)

  pure def maxInt(a: int, b: int): int = if (a > b) a else b
  pure def minInt(a: int, b: int): int = if (a < b) a else b

  // The k smallest (ascending) or k largest ids of s. Deterministic stand-in
  // for the Rust's seeded shuffles; the caller draws `ascending` nondeterministically
  // so both "Byzantine selected" and "Byzantine not selected" are explored.
  pure def takeK(s: Set[Validator], k: int, ascending: bool): Set[Validator] =
    s.filter(v =>
      (if (ascending) s.filter(u => u < v).size() else s.filter(u => u > v).size()) < k)

  // The oracle contract (DESIGN.md "The abstraction boundary"). Success only
  // with the ceremony's own fresh key; Failure with any subset of the
  // participants, restricted to Byzantine participants under StrongHonesty.
  pure def oracleOutcomes(
    cands: Set[Validator], freshKey: Key, byz: Set[Validator], strong: bool
  ): Set[Outcome] =
    val blameable = if (strong) byz.intersect(cands) else cands
    Set(Success(freshKey)).union(blameable.powerset().map(off => Failure(off)))

  run thresholdTest = all {
    // utilities/src/lib.rs check_threshold_calculation
    assert(thresholdOf(150) == 99),
    assert(successThreshold(150) == 100),
    assert(failureThreshold(150) == 51),
    assert(thresholdOf(4) == 2),
    assert(successThreshold(4) == 3),
    assert(failureThreshold(4) == 2),
    assert(thresholdOf(0) == 0),
  }

  run roundingTest = all {
    assert(percentOf(70, 4) == 3),   // 2.8 -> 3
    assert(percentOf(50, 5) == 2),   // 2.5 -> 2 (ties down)
    assert(percentOf(70, 10) == 7),
    assert(contractionFloor(4, 30) == 3),
    assert(contractionFloor(3, 30) == 2),  // 2.1 -> 2
  }

  run takeKTest = all {
    assert(takeK(Set(1, 2, 3, 4), 3, true) == Set(1, 2, 3)),
    assert(takeK(Set(1, 2, 3, 4), 3, false) == Set(2, 3, 4)),
    assert(takeK(Set(1, 2), 3, true) == Set(1, 2)),
    assert(takeK(Set(1, 2, 3), 0, true) == Set()),
  }

  run oracleTest = all {
    assert(oracleOutcomes(Set(1, 2, 3, 4), 7, Set(4), true)
      == Set(Success(7), Failure(Set()), Failure(Set(4)))),
    assert(oracleOutcomes(Set(1, 2, 3), 7, Set(4), true)
      == Set(Success(7), Failure(Set()))),
    assert(oracleOutcomes(Set(1, 2), 7, Set(4), false).contains(Failure(Set(1)))),
  }
}
