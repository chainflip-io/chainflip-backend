// Authority rotation orchestration.
//
// Mirrors cf-validator/src/lib.rs: RotationPhase, on_initialize,
// start_authority_rotation, start_keygen_attempt, try_restart_keygen,
// try_start_key_handover, abort_rotation, new_session, start_session,
// transition_to_next_epoch; helpers.rs select_sharing_participants; and the
// governance/safe-mode/broadcast gates. The auction is abstract.
module rotation {
  import types.* from "./types"
  import chain.* from "./chain"

  const VALIDATORS: Set[Validator]
  const BYZ: Set[Validator]
  const CHAINS: Chain -> ChainSpec
  const MIN_SIZE: int                // AuctionParameters.min_size
  const MAX_SIZE: int                // AuctionParameters.max_size; max_expansion treated as unbounded
  const CONTRACTION_PERCENT: int     // MaxAuthoritySetContractionPercentage
  const STRONG: bool                 // StrongHonesty for ceremony and signing attribution
  const FAIR: bool                   // fair scheduler (DESIGN.md "Liveness checking")
  const K_BLOCKS: int                // L1 bound: blocks from rotation start back to Idle

  pure val HONEST: Set[Validator] = VALIDATORS.exclude(BYZ)
  pure val CHAIN_IDS: Set[Chain] = CHAINS.keys()
  pure val INITIAL_EPOCH: Epoch = 1
  pure val GENESIS_KEY: Key = 1

  // RotationState minus bond.
  type RotationState = { primary: Set[Validator], banned: Set[Validator], newEpoch: Epoch }

  type Phase =
    | Idle
    | KeygensInProgress(RotationState)
    | KeyHandoversInProgress(RotationState)
    | ActivatingKeys(RotationState)
    | NewKeysActivated(RotationState)
    | SessionRotating(Set[Validator])

  // RotationError plus the other abort sites in on_initialize / try_start_key_handover.
  type AbortReason = AuctionFailed | NotEnoughCandidates | SharingUnavailable | SafeModeAtHandover | UnexpectedStatus

  type SharingRecord = { sharing: Set[Validator], current: Set[Validator], banned: Set[Validator], threshold: int }

  // History variables the Rust does not keep; used only by properties.
  type History = {
    keygenRestarts: int,
    handoverRetries: int,
    aborts: int,
    retryWithoutBan: bool,
    abortedFrom: Set[str],
    lastAbortReason: Option[AbortReason],
    lastRs: Option[RotationState],
    lastSharing: Option[SharingRecord],
    safeModeOffWhileActivating: bool,
    epochsAdvanced: int,
    rotationStartedAt: int,
  }

  type World = {
    phase: Phase,
    epoch: Epoch,
    authorities: Set[Validator],
    epochDue: bool,            // block_number - CurrentEpochStartedAt >= EpochDuration
    rotationEnabled: bool,     // SafeMode.authority_rotation_enabled
    broadcastsPending: bool,   // RotationBroadcastsPending::rotation_broadcasts_pending()
    bidders: Set[Validator],   // qualified bidders; bids abstracted to id order
    chains: Chain -> ChainState,
    nextKey: Key,              // fresh key for the next keygen (ceremony id)
    blocks: int,
    panics: Set[str],
    logErrors: Set[str],
    hist: History,
  }

  var w: World

  // ---------- pure helpers ----------

  pure def candidatesOf(rs: RotationState): Set[Validator] = rs.primary.exclude(rs.banned)

  pure def phaseName(p: Phase): str =
    match p {
    | Idle => "Idle"
    | KeygensInProgress(_) => "KeygensInProgress"
    | KeyHandoversInProgress(_) => "KeyHandoversInProgress"
    | ActivatingKeys(_) => "ActivatingKeys"
    | NewKeysActivated(_) => "NewKeysActivated"
    | SessionRotating(_) => "SessionRotating"
    }

  pure def isActivatingPhase(p: Phase): bool =
    match p {
    | ActivatingKeys(_) => true
    | NewKeysActivated(_) => true
    | SessionRotating(_) => true
    | _ => false
    }

  pure def somes(s: Set[Option[str]]): Set[str] =
    s.fold(Set(), (acc, o) => match o { | Some(x) => acc.union(Set(x)) | None => acc })

  // resolve_auction_iteratively, abstracted: the winners are the top
  // min(max_size, |qualified|) qualified bidders by id; NotEnoughBidders
  // below min_size. The environment varies the bidder set between auctions.
  pure def auction(x: World, excluded: Set[Validator]): Option[Set[Validator]] =
    val qualified = x.bidders.exclude(excluded)
    if (qualified.size() < MIN_SIZE) None
    else Some(takeK(qualified, minInt(MAX_SIZE, qualified.size()), true))

  // start_keygen_attempt: max(min_size, (1 - contraction) * current_authority_count)
  pure def sizeFloor(x: World): int =
    maxInt(MIN_SIZE, contractionFloor(x.authorities.size(), CONTRACTION_PERCENT))

  // helpers.rs select_sharing_participants: prefer old authorities that stay,
  // then old-only, up to the threshold. `ascending` replaces the seeded shuffle.
  pure def selectSharing(threshold: int, current: Set[Validator], cands: Set[Validator], ascending: bool): Option[Set[Validator]] =
    if (current.size() < threshold or cands == Set()) None
    else
      val both = current.intersect(cands)
      val oldOnly = current.exclude(cands)
      val fromBoth = minInt(threshold, both.size())
      Some(takeK(both, fromBoth, ascending).union(takeK(oldOnly, threshold - fromBoth, ascending)))

  pure def collect(x: World, steps: Chain -> ChainStep): World =
    { ...x,
      chains: CHAIN_IDS.mapBy(id => steps.get(id).chain),
      panics: x.panics.union(somes(CHAIN_IDS.map(id => steps.get(id).panic))),
      logErrors: x.logErrors.union(somes(CHAIN_IDS.map(id => steps.get(id).logError))) }

  pure def resetAll(x: World): World =
    { ...x, chains: CHAIN_IDS.mapBy(id => reset(x.chains.get(id))) }

  pure def progressAll(x: World): World =
    { ...x, chains: CHAIN_IDS.mapBy(id => progressActivation(x.chains.get(id))) }

  pure def keygenAll(x: World, cands: Set[Validator], epoch: Epoch, freshKey: Key): World =
    collect(x, CHAIN_IDS.mapBy(id => startKeygen(x.chains.get(id), cands, epoch, freshKey)))

  pure def handoverAll(x: World, sharing: Set[Validator], receiving: Set[Validator], epoch: Epoch): World =
    collect(x, CHAIN_IDS.mapBy(id => startHandover(x.chains.get(id), sharing, receiving, epoch)))

  pure def activateAll(x: World, newEpoch: Epoch, txFailed: Set[Chain], notRequired: Set[Chain]): World =
    collect(x, CHAIN_IDS.mapBy(id => activateKeys(x.chains.get(id), newEpoch,
      if (txFailed.contains(id)) TxFailed else if (notRequired.contains(id)) NotRequired else Normal)))

  pure def withLogError(x: World, site: str): World =
    { ...x, logErrors: x.logErrors.union(Set(site)) }

  // ---------- validator transitions ----------

  // abort_rotation(): reset every chain's key rotation, back to Idle.
  pure def abort(x: World, reason: AbortReason): World =
    val y = resetAll(x)
    { ...y, phase: Idle, hist: { ...y.hist,
        aborts: y.hist.aborts + 1,
        abortedFrom: y.hist.abortedFrom.union(Set(phaseName(x.phase))),
        lastAbortReason: Some(reason) } }

  // start_keygen_attempt()
  pure def startKeygenAttempt(x: World, rs: RotationState): World =
    val y = resetAll(x)
    val cands = candidatesOf(rs)
    if (cands.size() >= sizeFloor(y))
      val z = keygenAll(y, cands, rs.newEpoch, y.nextKey)
      { ...z, phase: KeygensInProgress(rs), nextKey: z.nextKey + 1 }
    else
      abort(y, NotEnoughCandidates)

  // try_restart_keygen(): ban, re-run the auction without the banned, restart.
  pure def tryRestartKeygen(x: World, rs: RotationState, offenders: Set[Validator]): World =
    val banned = rs.banned.union(offenders)
    val y = { ...x, hist: { ...x.hist,
      keygenRestarts: x.hist.keygenRestarts + 1,
      retryWithoutBan: x.hist.retryWithoutBan or banned == rs.banned } }
    match auction(y, banned) {
    | None => abort(y, AuctionFailed)
    | Some(winners) => startKeygenAttempt(y, { primary: winners, banned: banned, newEpoch: rs.newEpoch })
    }

  // try_start_key_handover()
  pure def tryStartKeyHandover(x: World, rs: RotationState, ascending: bool): World =
    if (not(x.rotationEnabled)) abort(x, SafeModeAtHandover)
    else
      val cands = candidatesOf(rs)
      val current = x.authorities.exclude(rs.banned)
      val threshold = successThreshold(x.authorities.size())
      match selectSharing(threshold, current, cands, ascending) {
      | None => abort(x, SharingUnavailable)
      | Some(sharing) =>
          val y = handoverAll(x, sharing, cands, rs.newEpoch)
          { ...y, phase: KeyHandoversInProgress(rs),
            hist: { ...y.hist, lastSharing: Some({
              sharing: sharing, current: current, banned: rs.banned, threshold: threshold }) } }
      }

  // start_authority_rotation()
  pure def startAuthorityRotation(x: World): World =
    if (not(x.rotationEnabled) or x.phase != Idle) x
    else match auction(x, Set()) {
      | None => abort(x, AuctionFailed)
      | Some(winners) =>
          val y = { ...x, hist: { ...x.hist, rotationStartedAt: x.blocks } }
          startKeygenAttempt(y, { primary: winners, banned: Set(), newEpoch: x.epoch + 1 })
    }

  // on_initialize, per phase. `st` is read after the status() side effects.
  pure def validatorHook(x0: World, txFailed: Set[Chain], notRequired: Set[Chain], ascending: bool): World =
    val x = progressAll(x0)
    val st = consStatus(x.chains)
    match x.phase {
    | Idle =>
        if (x.epochDue and not(x.broadcastsPending)) startAuthorityRotation(x) else x
    | KeygensInProgress(rs) =>
        match st {
        | Ready(o) => match o {
            | KeygenComplete => tryStartKeyHandover(x, rs, ascending)
            | FailedOuter(off) => tryRestartKeygen(x, rs, off)
            | _ => abort(withLogError(x, "unexpected_status_in_keygen"), UnexpectedStatus)
          }
        | Pending => x
        | Void => abort(withLogError(x, "unexpected_status_in_keygen"), UnexpectedStatus)
        }
    | KeyHandoversInProgress(rs) =>
        match st {
        | Ready(o) => match o {
            | KeyHandoverCompleteOuter =>
                val y = activateAll(x, rs.newEpoch, txFailed, notRequired)
                { ...y, phase: ActivatingKeys(rs) }
            | FailedOuter(off) =>
                if (off.intersect(candidatesOf(rs)) != Set())
                  tryRestartKeygen(x, rs, off)
                else
                  // Non-candidate offenders: ban and retry the handover with a fresh sharing set.
                  val rs2 = { ...rs, banned: rs.banned.union(off) }
                  val y = { ...x, hist: { ...x.hist,
                    handoverRetries: x.hist.handoverRetries + 1,
                    retryWithoutBan: x.hist.retryWithoutBan or off.subseteq(rs.banned) } }
                  tryStartKeyHandover(y, rs2, ascending)
            | _ => abort(withLogError(x, "unexpected_status_in_handover"), UnexpectedStatus)
          }
        | Pending => x
        | Void => abort(withLogError(x, "unexpected_status_in_handover"), UnexpectedStatus)
        }
    | ActivatingKeys(rs) =>
        match st {
        | Ready(o) => match o {
            | RotationComplete => { ...x, phase: NewKeysActivated(rs) }
            | _ => abort(withLogError(x, "unexpected_status_in_activation"), UnexpectedStatus)
          }
        | Pending => x
        | Void => abort(withLogError(x, "unexpected_status_in_activation"), UnexpectedStatus)
        }
    | _ => x
    }

  // pallet_session rotate_session, one call per block while should_end_session:
  // start_session (SessionRotating -> transition_to_next_epoch) then
  // new_session (NewKeysActivated -> SessionRotating; SessionRotating -> Idle).
  pure def sessionHook(x: World): World =
    match x.phase {
    | NewKeysActivated(rs) =>
        { ...x, phase: SessionRotating(candidatesOf(rs)), hist: { ...x.hist, lastRs: Some(rs) } }
    | SessionRotating(auths) =>
        { ...x, phase: Idle, epoch: x.epoch + 1, authorities: auths, epochDue: false,
          hist: { ...x.hist, epochsAdvanced: x.hist.epochsAdvanced + 1 } }
    | _ => x
    }

  pure def blockStep(x: World, txFailed: Set[Chain], notRequired: Set[Chain], ascending: bool): World =
    val y = sessionHook(validatorHook(x, txFailed, notRequired, ascending))
    { ...y, blocks: y.blocks + 1, hist: { ...y.hist,
        safeModeOffWhileActivating:
          y.hist.safeModeOffWhileActivating or (not(y.rotationEnabled) and isActivatingPhase(y.phase)) } }

  // ---------- init ----------

  action init = all {
    w' = {
      phase: Idle,
      epoch: INITIAL_EPOCH,
      authorities: VALIDATORS,
      epochDue: false,
      rotationEnabled: true,
      broadcastsPending: false,
      bidders: VALIDATORS,
      chains: CHAIN_IDS.mapBy(id => newChain(CHAINS.get(id),
        if (CHAINS.get(id).vault == Active) Some({ epoch: INITIAL_EPOCH, key: GENESIS_KEY }) else None)),
      nextKey: GENESIS_KEY + 1,
      blocks: 0,
      panics: Set(),
      logErrors: Set(),
      hist: {
        keygenRestarts: 0, handoverRetries: 0, aborts: 0, retryWithoutBan: false,
        abortedFrom: Set(), lastAbortReason: None, lastRs: None, lastSharing: None,
        safeModeOffWhileActivating: false, epochsAdvanced: 0, rotationStartedAt: 0,
      },
    }
  }

  // ---------- environment and adversary ----------

  // An environment delivery is still outstanding on some chain. The fair
  // scheduler lets no block pass while this holds.
  val envPending: bool = CHAIN_IDS.exists(id =>
    val c = w.chains.get(id)
    canApplyKeygenOutcome(c) or canApplyHandoverOutcome(c) or canApplyVerification(c)
      or canSignatureReady(c) or canGovernanceActivate(c))

  action block(txFailed: Set[Chain], notRequired: Set[Chain], ascending: bool): bool = all {
    not(FAIR and envPending),
    w' = blockStep(w, if (FAIR) Set() else txFailed, notRequired, ascending),
  }

  // Oracle outcome for one ceremony. Decodes the draw: Success if `succeed`,
  // else Failure with the drawn offenders intersected with the allowed set
  // (Byzantine participants under STRONG, any participant otherwise).
  // FAIR: an all-honest ceremony succeeds; a Byzantine failure is attributed.
  pure def decodeOutcome(cands: Set[Validator], freshKey: Key, succeed: bool, drawnOff: Set[Validator]): Outcome =
    val byzIn = BYZ.intersect(cands)
    if (FAIR)
      (if (byzIn == Set() or succeed) Success(freshKey) else Failure(byzIn))
    else if (succeed) Success(freshKey)
    else Failure(drawnOff.intersect(if (STRONG) byzIn else cands))

  action keygenOutcome(id: Chain, succeed: bool, drawnOff: Set[Validator]): bool =
    val c = w.chains.get(id)
    val k = keygenCeremony(c)
    all {
      canApplyKeygenOutcome(c),
      w' = { ...w, chains: w.chains.set(id,
        applyKeygenOutcome(c, decodeOutcome(k.cands, k.key, succeed, drawnOff))) },
    }

  action handoverOutcome(id: Chain, succeed: bool, drawnOff: Set[Validator]): bool =
    val c = w.chains.get(id)
    val h = handoverCeremony(c)
    all {
      canApplyHandoverOutcome(c),
      w' = { ...w, chains: w.chains.set(id,
        applyHandoverOutcome(c, decodeOutcome(h.participants, h.key, succeed, drawnOff))) },
    }

  // Verification signing result. Offenders come from the signing pallet's
  // blame logic: never honest under STRONG, possibly empty (offenders() drops
  // any set above half the candidates). FAIR: always Ok.
  action verificationResult(id: Chain, isOk: bool, drawnOff: Set[Validator]): bool =
    val c = w.chains.get(id)
    val parts = verificationParticipants(c)
    val off = drawnOff.intersect(if (STRONG) BYZ.intersect(parts) else parts)
    all {
      canApplyVerification(c),
      w' = { ...w, chains: w.chains.set(id, applyVerification(c, FAIR or isOk, off)) },
    }

  action signatureReadyOn(id: Chain): bool =
    val c = w.chains.get(id)
    all { canSignatureReady(c), w' = { ...w, chains: w.chains.set(id, signatureReady(c)) } }

  action governanceActivateOn(id: Chain): bool =
    val c = w.chains.get(id)
    all { canGovernanceActivate(c), w' = { ...w, chains: w.chains.set(id, governanceActivate(c)) } }

  action epochElapses = all { not(w.epochDue), w' = { ...w, epochDue: true } }

  action toggleSafeMode = all {
    not(FAIR),
    w' = { ...w, rotationEnabled: not(w.rotationEnabled) },
  }

  action toggleBroadcastsPending = all {
    not(FAIR),
    w' = { ...w, broadcastsPending: not(w.broadcastsPending) },
  }

  action bidderLeaves(v: Validator): bool = all {
    not(FAIR), w.bidders.contains(v),
    w' = { ...w, bidders: w.bidders.exclude(Set(v)) },
  }

  action bidderJoins(v: Validator): bool = all {
    not(FAIR), not(w.bidders.contains(v)),
    w' = { ...w, bidders: w.bidders.union(Set(v)) },
  }

  // force_rotation: governance, Idle, safe mode. It does NOT consult
  // RotationBroadcastsPending (on_initialize does).
  action forceRotation = all {
    w.phase == Idle, w.rotationEnabled,
    w' = startAuthorityRotation(w),
  }

  action step = {
    nondet id = CHAIN_IDS.oneOf()
    nondet v = VALIDATORS.oneOf()
    nondet txFailed = CHAIN_IDS.powerset().oneOf()
    nondet notRequired = CHAIN_IDS.powerset().oneOf()
    nondet ascending = Set(true, false).oneOf()
    nondet flag = Set(true, false).oneOf()
    nondet drawnOff = VALIDATORS.powerset().oneOf()
    any {
      block(txFailed, notRequired, ascending),
      keygenOutcome(id, flag, drawnOff),
      handoverOutcome(id, flag, drawnOff),
      verificationResult(id, flag, drawnOff),
      signatureReadyOn(id),
      governanceActivateOn(id),
      epochElapses,
      toggleSafeMode,
      toggleBroadcastsPending,
      bidderLeaves(v),
      bidderJoins(v),
      forceRotation,
    }
  }

  // Smoke witness for this task; the property set arrives in Task 6.
  val wEpochAdvanced = w.hist.epochsAdvanced >= 1
  val NoPanicsSmoke = w.panics == Set()
}
