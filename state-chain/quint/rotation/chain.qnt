// One chain's key rotation status and vault activation.
//
// Mirrors cf-threshold-signature/src/key_rotator.rs (KeyRotator impl),
// lib.rs (KeyRotationStatus, progress_rotation, on_key_verification_result,
// terminate_rotation), cf-vaults/src/vault_activator.rs (VaultActivator impl)
// and runtime/src/chainflip/cons_key_rotator.rs (ConsKeyRotator::status).
// Pure functions only; the state lives in validator.qnt's World.
module chain {
  import types.* from "./types"

  // cf-vaults PendingVaultActivation; NoActivation = storage item absent.
  type Activation = NoActivation | AwaitingActivation | ActivationComplete | AwaitingGovernance

  // KeyRotationStatus, all eleven variants, plus NotStarted for
  // PendingKeyRotation == None (status() == Void).
  type Status =
    | NotStarted
    | AwaitingKeygen({ cands: Set[Validator], epoch: Epoch, key: Key })
    | AwaitingKeygenVerification({ key: Key, participants: Set[Validator] })
    | KeygenVerificationComplete(Key)
    | AwaitingKeyHandover({ participants: Set[Validator], receiving: Set[Validator], epoch: Epoch, key: Key })
    | AwaitingKeyHandoverVerification({ key: Key, participants: Set[Validator] })
    | KeyHandoverComplete(Key)
    | AwaitingActivationSignatures
    | Complete
    | Failed(Set[Validator])
    | KeyHandoverFailed({ key: Key, offenders: Set[Validator] })

  type EpochKey = { epoch: Epoch, key: Key }

  type ChainState = {
    spec: ChainSpec,
    status: Status,
    // Keys[CurrentKeyEpoch] and CurrentKeyEpoch, i.e. active_epoch_key().
    activeKey: Option[EpochKey],
    activation: Activation,
    // AwaitingActivationSignatures.request_ids is non-empty.
    pendingSig: bool,
    // History for H2: this rotation ran a handover ceremony on this chain.
    handoverRan: bool,
  }

  // KeyRotationStatusOuter inside AsyncResult, as the validator sees them.
  type Outer = KeygenComplete | KeyHandoverCompleteOuter | RotationComplete | FailedOuter(Set[Validator])
  type Async = Void | Pending | Ready(Outer)

  // StartKeyActivationResult. FirstVault is folded into TxFailed (both wait
  // for governance); ChainNotInitialized is decided by spec.vault.
  type ActivationOutcome = Normal | NotRequired | TxFailed

  // A transition plus the assertion or error log the Rust would raise.
  type ChainStep = { chain: ChainState, panic: Option[str], logError: Option[str] }

  pure def ok(c: ChainState): ChainStep = { chain: c, panic: None, logError: None }
  pure def panicAt(c: ChainState, site: str): ChainStep = { chain: c, panic: Some(site), logError: None }
  pure def logErrorAt(c: ChainState, site: str): ChainStep = { chain: c, panic: None, logError: Some(site) }

  pure def newChain(spec: ChainSpec, genesisKey: Option[EpochKey]): ChainState = {
    spec: spec, status: NotStarted, activeKey: genesisKey,
    activation: NoActivation, pendingSig: false, handoverRan: false,
  }

  pure def withStatus(c: ChainState, s: Status): ChainState = { ...c, status: s }

  // key_rotator.rs status(). Assumes progressActivation already ran (the
  // Rust performs those side effects inside status()).
  pure def chainStatus(c: ChainState): Async =
    match c.status {
    | NotStarted => Void
    | AwaitingKeygen(_) => Pending
    | AwaitingKeygenVerification(_) => Pending
    | KeygenVerificationComplete(_) => Ready(KeygenComplete)
    | AwaitingKeyHandover(_) => Pending
    | AwaitingKeyHandoverVerification(_) => Pending
    | KeyHandoverComplete(_) => Ready(KeyHandoverCompleteOuter)
    | AwaitingActivationSignatures =>
        if (c.activation == ActivationComplete) Ready(RotationComplete) else Pending
    | Complete => Ready(RotationComplete)
    | Failed(off) => Ready(FailedOuter(off))
    | KeyHandoverFailed(f) => Ready(FailedOuter(f.offenders))
    }

  pure def isPending(c: ChainState): bool = chainStatus(c) == Pending
  pure def isComplete(c: ChainState): bool = c.status == Complete

  pure def offendersOf(o: Outer): Set[Validator] =
    match o { | FailedOuter(off) => off | _ => Set() }

  // cons_key_rotator.rs status(): Void dominates; any non-Ready is Pending;
  // equal Ready statuses pass through; mixed Ready is Failed with the union
  // of offenders, which is empty when neither side failed.
  pure def consPair(a: Async, b: Async): Async =
    match a {
    | Void => Void
    | Pending => match b { | Void => Void | _ => Pending }
    | Ready(x) => match b {
        | Void => Void
        | Pending => Pending
        | Ready(y) =>
            if (x == y) Ready(x) else Ready(FailedOuter(offendersOf(x).union(offendersOf(y))))
      }
    }

  // Fold order does not matter: consPair is associative and commutative.
  pure def consStatus(chains: Chain -> ChainState): Async =
    val folded = chains.keys().fold(None, (acc, id) =>
      match acc {
      | None => Some(chainStatus(chains.get(id)))
      | Some(a) => Some(consPair(a, chainStatus(chains.get(id))))
      })
    match folded { | None => Void | Some(a) => a }

  // key_rotator.rs keygen(): asserts non-empty candidates and status != Pending.
  pure def startKeygen(c: ChainState, cands: Set[Validator], epoch: Epoch, freshKey: Key): ChainStep =
    if (cands == Set()) panicAt(c, "keygen_empty_candidates")
    else if (isPending(c)) panicAt(c, "keygen_while_pending")
    else ok({ ...c,
      status: AwaitingKeygen({ cands: cands, epoch: epoch, key: freshKey }),
      handoverRan: false })

  pure def canApplyKeygenOutcome(c: ChainState): bool =
    match c.status { | AwaitingKeygen(_) => true | _ => false }

  pure def keygenCeremony(c: ChainState): { cands: Set[Validator], epoch: Epoch, key: Key } =
    match c.status { | AwaitingKeygen(k) => k | _ => { cands: Set(), epoch: 0, key: 0 } }

  // progress_rotation on AwaitingKeygen: success -> trigger_keygen_verification,
  // failure -> terminate_rotation -> Failed.
  pure def applyKeygenOutcome(c: ChainState, o: Outcome): ChainState =
    val k = keygenCeremony(c)
    match o {
    | Success(key) => withStatus(c, AwaitingKeygenVerification({ key: key, participants: k.cands }))
    | Failure(off) => withStatus(c, Failed(off))
    }

  pure def canApplyVerification(c: ChainState): bool =
    match c.status {
    | AwaitingKeygenVerification(_) => true
    | AwaitingKeyHandoverVerification(_) => true
    | _ => false
    }

  pure def verificationParticipants(c: ChainState): Set[Validator] =
    match c.status {
    | AwaitingKeygenVerification(v) => v.participants
    | AwaitingKeyHandoverVerification(v) => v.participants
    | _ => Set()
    }

  // on_keygen_verification_result / on_handover_verification_result:
  // Ok -> the *Complete status; Err(offenders) -> terminate_rotation -> Failed.
  pure def applyVerification(c: ChainState, isOk: bool, off: Set[Validator]): ChainState =
    match c.status {
    | AwaitingKeygenVerification(v) =>
        withStatus(c, if (isOk) KeygenVerificationComplete(v.key) else Failed(off))
    | AwaitingKeyHandoverVerification(v) =>
        withStatus(c, if (isOk) KeyHandoverComplete(v.key) else Failed(off))
    | _ => c
    }

  // key_rotator.rs key_handover().
  pure def startHandover(c: ChainState, sharing: Set[Validator], receiving: Set[Validator], epoch: Epoch): ChainStep =
    if (isPending(c)) panicAt(c, "handover_while_pending")
    else
      val fromKey = match c.status {
        | KeygenVerificationComplete(k) => Some(k)
        | KeyHandoverFailed(f) => Some(f.key)
        | _ => None
      }
      match fromKey {
      | Some(k) =>
          if (c.spec.kind == Utxo and c.activeKey != None)
            if (sharing == Set() or receiving == Set()) panicAt(c, "handover_empty_participants")
            else ok({ ...c,
              status: AwaitingKeyHandover({
                participants: sharing.union(receiving), receiving: receiving, epoch: epoch, key: k }),
              handoverRan: true })
          else
            // NoKeyHandover: not a UTXO chain, or no key to hand over yet.
            ok(withStatus(c, KeyHandoverComplete(k)))
      | None => match c.status {
          | KeyHandoverComplete(_) => ok(c)   // "Key handover already complete."
          | _ => panicAt(c, "handover_invalid_state")   // log_or_panic!
        }
      }

  pure def canApplyHandoverOutcome(c: ChainState): bool =
    match c.status { | AwaitingKeyHandover(_) => true | _ => false }

  pure def handoverCeremony(c: ChainState): { participants: Set[Validator], receiving: Set[Validator], epoch: Epoch, key: Key } =
    match c.status {
    | AwaitingKeyHandover(h) => h
    | _ => { participants: Set(), receiving: Set(), epoch: 0, key: 0 }
    }

  // progress_rotation on AwaitingKeyHandover: success -> handover verification
  // signed by the receiving participants; failure -> KeyHandoverFailed.
  pure def applyHandoverOutcome(c: ChainState, o: Outcome): ChainState =
    val h = handoverCeremony(c)
    match o {
    | Success(key) => withStatus(c, AwaitingKeyHandoverVerification({ key: key, participants: h.receiving }))
    | Failure(off) => withStatus(c, KeyHandoverFailed({ key: h.key, offenders: off }))
    }

  // key_rotator.rs activate_keys() + vault_activator.rs start_key_activation().
  // The next-epoch key is written before activation completes (set_key_for_epoch).
  pure def activateKeys(c: ChainState, newEpoch: Epoch, outcome: ActivationOutcome): ChainStep =
    match c.status {
    | KeyHandoverComplete(k) =>
        if (c.spec.vault == Uninitialised)
          ok(withStatus(c, Complete))   // ChainNotInitialized: no key written
        else
          val keyed = { ...c, activeKey: Some({ epoch: newEpoch, key: k }) }
          match outcome {
          | Normal => ok({ ...keyed,
              status: AwaitingActivationSignatures, activation: AwaitingActivation, pendingSig: true })
          | NotRequired => ok({ ...keyed,
              status: Complete, activation: ActivationComplete, pendingSig: false })
          | TxFailed => ok({ ...keyed,
              status: AwaitingActivationSignatures, activation: AwaitingGovernance, pendingSig: false })
          }
    | _ => logErrorAt(c, "activate_wrong_state")   // log::error!, not a panic
    }

  // status() side effects on AwaitingActivationSignatures: once no request id
  // is outstanding, activate_key() completes the vault unless it awaits
  // governance; a Ready vault marks the key rotation complete.
  pure def progressActivation(c: ChainState): ChainState =
    match c.status {
    | AwaitingActivationSignatures =>
        if (c.pendingSig) c
        else match c.activation {
          | AwaitingActivation => { ...c, activation: ActivationComplete, status: Complete }
          | ActivationComplete => withStatus(c, Complete)
          | _ => c
        }
    | _ => c
    }

  pure def canGovernanceActivate(c: ChainState): bool = c.activation == AwaitingGovernance
  // vault_key_rotated_externally / on_first_key_activated.
  pure def governanceActivate(c: ChainState): ChainState = { ...c, activation: ActivationComplete }

  pure def canSignatureReady(c: ChainState): bool = c.pendingSig
  pure def signatureReady(c: ChainState): ChainState = { ...c, pendingSig: false }

  // reset_key_rotation(): kills PendingKeyRotation only; the vault activation
  // status and the epoch keys survive.
  pure def reset(c: ChainState): ChainState = withStatus(c, NotStarted)

  pure def isFailedStatus(c: ChainState): bool =
    match c.status { | Failed(_) => true | KeyHandoverFailed(_) => true | _ => false }

  pure def statusOffenders(c: ChainState): Set[Validator] =
    match c.status { | Failed(off) => off | KeyHandoverFailed(f) => f.offenders | _ => Set() }

  // --- tests on hand-built states ---

  pure val BTC: ChainSpec = { kind: Utxo, vault: Active }
  pure val EVM: ChainSpec = { kind: NonUtxo, vault: Active }
  pure val SOL_UNINIT: ChainSpec = { kind: NonUtxo, vault: Uninitialised }
  pure val GENESIS: Option[EpochKey] = Some({ epoch: 1, key: 1 })
  pure val ALL4: Set[Validator] = Set(1, 2, 3, 4)

  run consPairTest = all {
    assert(consPair(Ready(KeygenComplete), Ready(KeygenComplete)) == Ready(KeygenComplete)),
    assert(consPair(Ready(KeygenComplete), Ready(FailedOuter(Set(4)))) == Ready(FailedOuter(Set(4)))),
    assert(consPair(Ready(FailedOuter(Set(1))), Ready(FailedOuter(Set(4)))) == Ready(FailedOuter(Set(1, 4)))),
    // Mixed non-failed Ready statuses collapse to an EMPTY failure (H4 territory).
    assert(consPair(Ready(KeygenComplete), Ready(RotationComplete)) == Ready(FailedOuter(Set()))),
    assert(consPair(Pending, Ready(KeygenComplete)) == Pending),
    assert(consPair(Void, Pending) == Void),
    assert(consPair(Ready(KeygenComplete), Void) == Void),
  }

  run utxoHappyPathTest =
    val c0 = newChain(BTC, GENESIS)
    val c1 = startKeygen(c0, ALL4, 2, 7).chain
    val c2 = applyKeygenOutcome(c1, Success(7))
    val c3 = applyVerification(c2, true, Set())
    val c4 = startHandover(c3, Set(1, 2, 3), ALL4, 2).chain
    val c5 = applyHandoverOutcome(c4, Success(7))
    val c6 = applyVerification(c5, true, Set())
    val c7 = activateKeys(c6, 2, Normal).chain
    val c8 = progressActivation(signatureReady(c7))
    all {
      assert(chainStatus(c1) == Pending),
      assert(chainStatus(c3) == Ready(KeygenComplete)),
      assert(c4.handoverRan),
      assert(handoverCeremony(c4).participants == ALL4),
      assert(verificationParticipants(c5) == ALL4),
      assert(chainStatus(c6) == Ready(KeyHandoverCompleteOuter)),
      assert(chainStatus(c7) == Pending),
      assert(c7.activeKey == Some({ epoch: 2, key: 7 })),
      assert(chainStatus(c8) == Ready(RotationComplete)),
      assert(isComplete(c8)),
    }

  run nonUtxoSkipsHandoverTest =
    val c = startHandover(applyVerification(applyKeygenOutcome(
      startKeygen(newChain(EVM, GENESIS), ALL4, 2, 7).chain, Success(7)), true, Set()), Set(1, 2, 3), ALL4, 2)
    all {
      assert(c.panic == None),
      assert(c.chain.status == KeyHandoverComplete(7)),
      assert(not(c.chain.handoverRan)),
    }

  run utxoWithoutKeySkipsHandoverTest =
    val c = startHandover(applyVerification(applyKeygenOutcome(
      startKeygen(newChain(BTC, None), ALL4, 2, 7).chain, Success(7)), true, Set()), Set(1, 2, 3), ALL4, 2)
    all { assert(c.chain.status == KeyHandoverComplete(7)) }

  run handoverOnFailedChainPanicsTest =
    val failed = withStatus(newChain(BTC, GENESIS), Failed(Set()))
    all { assert(startHandover(failed, Set(1, 2, 3), ALL4, 2).panic == Some("handover_invalid_state")) }

  run handoverAlreadyCompleteIsNoopTest =
    val done = withStatus(newChain(EVM, GENESIS), KeyHandoverComplete(7))
    val r = startHandover(done, Set(1, 2, 3), ALL4, 2)
    all { assert(r.panic == None), assert(r.chain == done) }

  run keygenWhilePendingPanicsTest =
    val pending = startKeygen(newChain(BTC, GENESIS), ALL4, 2, 7).chain
    all {
      assert(startKeygen(pending, ALL4, 2, 8).panic == Some("keygen_while_pending")),
      assert(startKeygen(newChain(BTC, GENESIS), Set(), 2, 8).panic == Some("keygen_empty_candidates")),
      assert(startKeygen(reset(pending), ALL4, 2, 8).panic == None),
    }

  run uninitialisedCompletesWithoutKeyTest =
    val c = activateKeys(withStatus(newChain(SOL_UNINIT, None), KeyHandoverComplete(7)), 2, Normal).chain
    all { assert(isComplete(c)), assert(c.activeKey == None) }

  run txFailedWaitsForGovernanceTest =
    val c = activateKeys(withStatus(newChain(EVM, GENESIS), KeyHandoverComplete(7)), 2, TxFailed).chain
    all {
      assert(chainStatus(progressActivation(c)) == Pending),
      assert(c.activeKey == Some({ epoch: 2, key: 7 })),
      assert(chainStatus(progressActivation(governanceActivate(c))) == Ready(RotationComplete)),
    }

  run verificationFailureTerminatesTest =
    val kv = applyKeygenOutcome(startKeygen(newChain(BTC, GENESIS), ALL4, 2, 7).chain, Success(7))
    all {
      assert(applyVerification(kv, false, Set()).status == Failed(Set())),
      assert(applyVerification(kv, false, Set(4)).status == Failed(Set(4))),
    }
}
