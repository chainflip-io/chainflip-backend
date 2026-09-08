# Quint Authority Rotation Model — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A layered Quint model of Chainflip authority rotation (validator phases × per-chain key rotation × vault activation) whose safety, panic-freedom and bounded-liveness properties are simulated and exhaustively verified on a four-validator, two-chain instance, with negative controls and must-reach witnesses.

**Architecture:** Five modules in `state-chain/quint/rotation/`. `ceremony.qnt` is a concrete model of on-chain ceremony vote resolution, verified alone and discharging an oracle contract. `chain.qnt` is pure functions over one chain's `KeyRotationStatus`. `validator.qnt` holds the single `World` state variable, the block hook, environment actions, and all upper-layer properties. `harness.qnt` instantiates the `const`s (`main`, `uninit`, `split`, `fair`). `check.sh` runs everything.

**Tech Stack:** Quint 0.32.0 (`quint typecheck | test | run | verify`), Apalache 0.56.1 (auto-downloaded to `~/.quint`, needs a JDK; OpenJDK 26 is installed). No Rust changes.

**Spec:** `state-chain/quint/rotation/DESIGN.md` — the plan argues from the spec; read both. Property identifiers (C1–C3, H2–H4, PF1–PF4, R1–R7, NC1–NC3, L1–L3, W1–W6) are the spec's and are used verbatim in code.

## Global Constraints

- Quint 0.32.0. Verify with `quint --version` before starting. Apalache is fetched on first `quint verify`.
- Every ceremony/chain/validator transition cites the Rust it mirrors in a comment (`// key_rotator.rs: key_handover`). Line numbers are not required; function names are.
- **Never** write `setOfMaps(D, C).oneOf()`; Apalache rejects it at verify time only. Draw from `powerset()` and decode.
- **Never** draw a `nondet` value freely and then constrain it in `all { }`. Decode every draw into a valid value (intersect with the allowed set, `if FAIR then … else …`). Constraining disables the transition and blows up Apalache cost (keygen model measured 2.6× per step).
- Every `var` is assigned in every action. There is exactly one `var` in `validator.qnt` (`w: World`) so this is one assignment per action.
- `quint test` only runs `run` definitions whose name ends in `Test`. Name every test `…Test`. Never use `--match '.*'`: it evaluates every definition (types, actions, vals) as a test and reports spurious failures.
- Multi-module files need `--main`. Every `quint run`/`verify`/`test` on `harness.qnt` or `ceremony.qnt` passes `--main`.
- Piped REPL input needs `--backend=typescript`.
- `Option` is not built in: `types.qnt` defines `type Option[a] = Some(a) | None`.
- `fail`, `to`, `from`, `val`, `def` are reserved: do not use them as identifiers or field names.
- Commit messages: `feat:` for model code, `doc:` for README/DESIGN/PLAN, `chore:` for `check.sh`. No `Co-Authored-By` trailer. Sign normally; if the YubiKey is absent, use `--no-gpg-sign` and say so.
- Thresholds are copied from `utilities/src/lib.rs`: `t(n) = (2n − 1) / 3` (`t(0) = 0`), success `t + 1`, failure `n − t`. `Percent * u32` rounds to nearest, ties down (`sp_arithmetic` `Rounding::NearestPrefDown`), so the 70% floor at n=4 is **3**.

## File structure

| File | Responsibility | Task |
| --- | --- | --- |
| `types.qnt` | ids, `Option`, chain tags, `Outcome`, threshold and rounding arithmetic, `takeK`, `oracleOutcomes` | 1 |
| `ceremony.qnt` | module `ceremony`: `ResponseStatus`, `addReport`, `resolve`, `dropAtFailureThreshold`, `resolveKeygen`; module `ceremonyCheck`: one-ceremony state machine, C1–C3, `SeamSound`, NC2 target, witnesses | 2, 3 |
| `chain.qnt` | module `chain`: `ChainState`, `Status`, `chainStatus`, `consStatus`, all per-chain transitions returning `ChainStep` | 4 |
| `validator.qnt` | module `rotation`: consts, `World`, hooks, environment actions, `step`, R1–R7, H2–H4, PF, NC3, L1/L2, W1–W6 | 5, 6 |
| `harness.qnt` | instances `main`, `uninit`, `split`, `fair`, `main1`, `ceremonyStrong`, `ceremonySplit`, `ceremonyOutage`; NC aliases; deterministic `…Test` runs | 3, 6, 7, 7b |
| `check.sh` | typecheck, tests, simulation, MUST_VIOLATE, `--verify` | 8 |
| `README.md` | setup, running, status table, witness counts, gotchas, known gaps, findings | 1, 8, 9 |

Read order for a fresh executor: `DESIGN.md` → `types.qnt` → `ceremony.qnt` → `chain.qnt` → `validator.qnt` → `harness.qnt`.

---

### Task 1: Scaffold and `types.qnt`

**Files:**
- Create: `state-chain/quint/rotation/types.qnt`
- Create: `state-chain/quint/rotation/README.md` (skeleton)
- Create: `state-chain/quint/rotation/check.sh` (typecheck-only skeleton; Task 8 completes it)

**Interfaces:**
- Produces: `Validator = int`, `Chain = str`, `Key = int`, `Epoch = int`, `Option[a]`, `ChainKind`, `Vault`, `ChainSpec`, `Outcome`, `thresholdOf(n)`, `successThreshold(n)`, `failureThreshold(n)`, `percentOf(percent, n)`, `contractionFloor(current, contractionPercent)`, `maxInt`, `minInt`, `takeK(s, k, ascending)`, `oracleOutcomes(cands, freshKey, byz, strong)`.

- [ ] **Step 1: Write `types.qnt`**

```quint
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
```

- [ ] **Step 2: Typecheck and run the tests**

Run: `cd state-chain/quint/rotation && quint typecheck types.qnt && quint test types.qnt`
Expected: `ok thresholdTest`, `ok roundingTest`, `ok takeKTest`, `ok oracleTest`, `4 passing`.

- [ ] **Step 3: Write the README skeleton**

```markdown
# Quint model of authority rotation stages

Formal model of the rotation orchestration in `cf-validator`, the per-chain
key rotation status in `cf-threshold-signature`, the multi-chain combinator
in `runtime/src/chainflip/cons_key_rotator.rs`, and vault activation in
`cf-vaults`. See `DESIGN.md` for scope, properties and the oracle contract.

## Setup

```bash
npm install -g @informalsystems/quint    # 0.32.0 or later
quint --version
```

Apalache (for `quint verify`) and the Rust evaluator are downloaded into
`~/.quint` on first use. Apalache needs a JDK.

## Running

```bash
./check.sh            # typecheck, tests, simulation, negative controls (~1 min)
./check.sh --verify   # add exhaustive Apalache checks (budget ~15 min)
```

Every module except `types.qnt` declares `const`s, so checks target an
instance in `harness.qnt` via `--main`:

```bash
quint run harness.qnt --main=main --invariant=PF_NoPanics --max-steps=40
quint test harness.qnt --main=main
```

## Status

(filled in by Task 8)

## Gotchas

- Never write `setOfMaps(D, C).oneOf()`; Apalache rejects it, `quint run` does not.
- Draw `nondet` values from `powerset()` and decode them; never constrain a
  free draw inside `all { }`.
- `Option` is not in the standard library; `types.qnt` defines it.
- Without `--main`, quint picks the module named after the file, which is
  wrong for every multi-module file here.
- `fail` is a built-in; do not name an action `fail`.

## Known gaps

(filled in by Task 8)
```

- [ ] **Step 4: Write the check.sh skeleton**

```bash
#!/usr/bin/env bash
# Run every Quint check for the rotation model.
#   ./check.sh          simulation only
#   ./check.sh --verify add exhaustive Apalache checks
set -euo pipefail
cd "$(dirname "$0")"

command -v quint >/dev/null || { echo "quint not on PATH; see README.md"; exit 1; }

echo "== typecheck =="
# `if quint typecheck; then` rather than `&&`: under set -e a failing command
# left of && does not abort, and a check script that exits 0 on failure is
# worse than none.
for f in types.qnt; do
  if quint typecheck "$f"; then echo "  ok $f"; else echo "  FAILED $f"; exit 1; fi
done

echo "== unit tests =="
quint test types.qnt
```

Run: `chmod +x check.sh && ./check.sh`
Expected: `ok types.qnt` and `4 passing`.

- [ ] **Step 5: Commit**

```bash
git add state-chain/quint/rotation/types.qnt state-chain/quint/rotation/README.md state-chain/quint/rotation/check.sh
git commit -m "feat: scaffold the Quint rotation model with shared types (PRO-3120)"
```

---

### Task 2: `ceremony.qnt` pure layer

**Files:**
- Create: `state-chain/quint/rotation/ceremony.qnt` (module `ceremony`)
- Modify: `state-chain/quint/rotation/check.sh` (add `ceremony.qnt` to the typecheck loop and `quint test ceremony.qnt --main=ceremony`)

**Interfaces:**
- Consumes: `types.*`
- Produces: `Report = SuccessVote(Key) | FailureVote(Set[Validator])`, `ResponseStatus`, `newResponseStatus(cands)`, `canReport(rs, v)`, `addReport(rs, v, r)`, `votersFor(rs, k)`, `resolve(rs): Outcome`, `dropAtFailureThreshold(o, n)`, `resolveKeygen(rs): Outcome`, `resolveHandover(rs, expectedKey): Outcome`.

- [ ] **Step 1: Write module `ceremony`**

```quint
// On-chain resolution of one keygen or key-handover ceremony.
//
// Mirrors cf-threshold-signature/src/response_status.rs (ResponseStatus,
// resolve_keygen_outcome) and the failure-threshold drop in lib.rs
// progress_rotation. Verified standalone (module ceremonyCheck); the chain
// and validator layers consume outcomes through types.oracleOutcomes.
module ceremony {
  import types.* from "./types"

  // report_keygen_outcome / report_key_handover_outcome payload.
  type Report = SuccessVote(Key) | FailureVote(Set[Validator])

  type ResponseStatus = {
    candidates: Set[Validator],
    remaining: Set[Validator],
    successVotes: Set[{ voter: Validator, key: Key }],
    failureVoters: Set[Validator],
    blames: Set[{ voter: Validator, blamed: Validator }],
  }

  pure def newResponseStatus(cands: Set[Validator]): ResponseStatus = {
    candidates: cands,
    remaining: cands,
    successVotes: Set(),
    failureVoters: Set(),
    blames: Set(),
  }

  // handle_key_ceremony_report!: only a remaining candidate may report, once.
  pure def canReport(rs: ResponseStatus, v: Validator): bool = rs.remaining.contains(v)

  // add_success_vote / add_failure_vote. Blames outside the candidate set are
  // dropped (handle_key_ceremony_report! partitions them off with a warning).
  pure def addReport(rs: ResponseStatus, v: Validator, r: Report): ResponseStatus =
    match r {
    | SuccessVote(k) => { ...rs,
        remaining: rs.remaining.exclude(Set(v)),
        successVotes: rs.successVotes.union(Set({ voter: v, key: k })) }
    | FailureVote(blamed) => { ...rs,
        remaining: rs.remaining.exclude(Set(v)),
        failureVoters: rs.failureVoters.union(Set(v)),
        blames: rs.blames.union(
          blamed.intersect(rs.candidates).map(b => { voter: v, blamed: b })) }
    }

  pure def votersFor(rs: ResponseStatus, k: Key): Set[Validator] =
    rs.successVotes.filter(s => s.key == k).map(s => s.voter)

  pure def votedKeys(rs: ResponseStatus): Set[Key] = rs.successVotes.map(s => s.key)

  pure def blameCount(rs: ResponseStatus, b: Validator): int =
    rs.blames.filter(x => x.blamed == b).size()

  pure def superMajority(rs: ResponseStatus): int = successThreshold(rs.candidates.size())

  // resolve_keygen_outcome, without final_key_check (see resolveHandover).
  //   all candidates on one key            -> Success
  //   some key at the super-majority        -> punish other success voters + failure voters
  //   else failure voters at super-majority -> punish all success voters
  //   else                                  -> punish no voter
  //   plus: heavily blamed (>= super-majority blames) and non-responders.
  pure def resolve(rs: ResponseStatus): Outcome =
    val n = rs.candidates.size()
    val sm = superMajority(rs)
    val unanimous = votedKeys(rs).filter(k => votersFor(rs, k).size() == n)
    if (unanimous != Set())
      Success(unanimous.getOnlyElement())
    else
      // At most one key can reach the super-majority (t + 1 > n / 2).
      val exemptKeys = votedKeys(rs).filter(k => votersFor(rs, k).size() >= sm)
      val punishedVoters =
        if (exemptKeys != Set())
          rs.successVotes.filter(s => not(exemptKeys.contains(s.key))).map(s => s.voter)
            .union(rs.failureVoters)
        else if (rs.failureVoters.size() >= sm)
          rs.successVotes.map(s => s.voter)
        else
          Set()
      val heavilyBlamed = rs.candidates.filter(b => blameCount(rs, b) >= sm)
      Failure(punishedVoters.union(heavilyBlamed).union(rs.remaining))

  // progress_rotation: an offender set at or above the failure threshold is
  // dropped to empty ("we don't want to risk liveness"), so nobody is banned.
  pure def dropAtFailureThreshold(o: Outcome, n: int): Outcome =
    match o {
    | Failure(off) => if (off.size() < failureThreshold(n)) Failure(off) else Failure(Set())
    | Success(k) => Success(k)
    }

  pure def resolveKeygen(rs: ResponseStatus): Outcome =
    dropAtFailureThreshold(resolve(rs), rs.candidates.size())

  // Handover adds final_key_check: handover_key_matches(current, reported)
  // must hold, else the outcome is Err(Default::default()) = Failure(Set()).
  pure def resolveHandover(rs: ResponseStatus, expectedKey: Key): Outcome =
    match resolve(rs) {
    | Success(k) => if (k == expectedKey) Success(k) else Failure(Set())
    | Failure(off) => dropAtFailureThreshold(Failure(off), rs.candidates.size())
    }

  // --- tests (response_status.rs tests, progress_rotation behaviour) ---

  pure val C4: Set[Validator] = Set(1, 2, 3, 4)

  pure def reportAll(rs: ResponseStatus, voters: Set[Validator], r: Report): ResponseStatus =
    voters.fold(rs, (acc, v) => addReport(acc, v, r))

  run unanimitySucceedsTest = all {
    assert(resolveKeygen(reportAll(newResponseStatus(C4), C4, SuccessVote(7))) == Success(7)),
  }

  run dissenterIsPunishedTest =
    val rs = addReport(reportAll(newResponseStatus(C4), Set(1, 2, 3), SuccessVote(7)), 4, SuccessVote(8))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run failureVoterAgainstSuperMajorityIsPunishedTest =
    val rs = addReport(reportAll(newResponseStatus(C4), Set(1, 2, 3), SuccessVote(7)), 4, FailureVote(Set()))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run successVoterAgainstFailureSuperMajorityIsPunishedTest =
    val rs = addReport(reportAll(newResponseStatus(C4), Set(1, 2, 3), FailureVote(Set())), 4, SuccessVote(7))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run noConsensusPunishesNoVoterTest =
    // 2 vs 2: neither side reaches the super-majority of 3.
    val rs = reportAll(reportAll(newResponseStatus(C4), Set(1, 2), SuccessVote(7)), Set(3, 4), FailureVote(Set()))
    all { assert(resolveKeygen(rs) == Failure(Set())) }

  run heavilyBlamedIsPunishedTest =
    // 4 is blamed three times (>= 3) and is also a non-responder.
    val rs = reportAll(newResponseStatus(C4), Set(1, 2, 3), FailureVote(Set(4)))
    all {
      assert(resolve(rs) == Failure(Set(4))),
      assert(resolveKeygen(rs) == Failure(Set(4))),
    }

  run blamesOutsideCandidatesAreDroppedTest =
    val rs = reportAll(newResponseStatus(C4), Set(1, 2, 3), FailureVote(Set(9)))
    all { assert(rs.blames == Set()) }

  run failureThresholdDropTest =
    // Two non-responders at n=4: |off| = 2 >= failure threshold 2 -> dropped.
    val rs = reportAll(newResponseStatus(C4), Set(1, 2), SuccessVote(7))
    all {
      assert(resolve(rs) == Failure(Set(3, 4))),
      assert(resolveKeygen(rs) == Failure(Set())),
    }

  run singleNonResponderIsKeptTest =
    val rs = reportAll(newResponseStatus(C4), Set(1, 2, 3), SuccessVote(7))
    all { assert(resolveKeygen(rs) == Failure(Set(4))) }

  run handoverKeyMismatchIsUnattributedTest = all {
    assert(resolveHandover(reportAll(newResponseStatus(C4), C4, SuccessVote(7)), 7) == Success(7)),
    assert(resolveHandover(reportAll(newResponseStatus(C4), C4, SuccessVote(8)), 7) == Failure(Set())),
  }
}
```

- [ ] **Step 2: Typecheck and run the tests**

Run: `cd state-chain/quint/rotation && quint typecheck ceremony.qnt && quint test ceremony.qnt --main=ceremony`
Expected: 10 passing. If `noConsensusPunishesNoVoterTest` fails, re-read `resolve_keygen_outcome` in `response_status.rs`: the three-way branch punishes no voter when neither a key nor the failure voters reach the super-majority.

- [ ] **Step 3: Add to check.sh**

Change the typecheck loop to `for f in types.qnt ceremony.qnt; do` and append under unit tests:

```bash
quint test ceremony.qnt --main=ceremony
```

Run: `./check.sh` — Expected: both files `ok`, 14 tests passing in total.

- [ ] **Step 4: Commit**

```bash
git add state-chain/quint/rotation/ceremony.qnt state-chain/quint/rotation/check.sh
git commit -m "feat: model on-chain ceremony vote resolution (PRO-3120)"
```

---

### Task 3: `ceremonyCheck` state machine, C1–C3, seam, NC1, NC2

**Files:**
- Modify: `state-chain/quint/rotation/ceremony.qnt` (append module `ceremonyCheck`)
- Create: `state-chain/quint/rotation/harness.qnt` (instances `ceremonyStrong`, `ceremonySplit`, `ceremonyOutage`)
- Modify: `state-chain/quint/rotation/check.sh`

**Interfaces:**
- Consumes: `ceremony.*`, `types.oracleOutcomes`
- Produces: `ceremonyCheck` consts `CANDS`, `BYZ`, `STRONG`, `HONEST_MAY_TIMEOUT`; invariants `C1_AcceptanceUnanimity`, `C2_OffendersAreParticipants`, `C3_HonestNeverOffender`, `SeamSound`, `NC2_DropNeverEmpties_MustFailHere`; witnesses `wResolvedSuccess`, `wResolvedFailure`, `wByzantinePunished`, `wDropped`.

Why three instances: with one Byzantine at n=4 and honest nodes reporting before the timeout, the offender set can never reach the failure threshold of 2, so NC2 needs an instance where honest nodes may time out (an outage, which is the real-world trigger for the drop). NC1 needs the split.

- [ ] **Step 1: Append module `ceremonyCheck` to `ceremony.qnt`**

```quint
// One ceremony driven to resolution by honest and Byzantine reports.
// Verified standalone; SeamSound discharges the oracle contract the chain
// and validator layers assume.
module ceremonyCheck {
  import types.* from "./types"
  import ceremony.* from "./ceremony"

  const CANDS: Set[Validator]
  const BYZ: Set[Validator]
  // StrongHonesty (DESIGN.md): every honest candidate reports the same outcome.
  // false = the keygen model's split (some honest Agreed, others Failed).
  const STRONG: bool
  // false = honest nodes report before KeygenResponseTimeout (the default
  // assumption). true = an outage: the timeout may fire with honest nodes
  // still pending, which is what makes the failure-threshold drop reachable.
  const HONEST_MAY_TIMEOUT: bool

  pure val HONEST: Set[Validator] = CANDS.exclude(BYZ)
  pure val FRESH_KEY: Key = 7
  pure val OTHER_KEY: Key = 8

  // What an honest node can report: the fresh key, or a failure blaming a
  // subset of the Byzantine candidates (empty = unattributed). Keygen model K1/K3.
  pure val HONEST_REPORTS: Set[Report] =
    Set(SuccessVote(FRESH_KEY))
      .union(BYZ.intersect(CANDS).powerset().map(b => FailureVote(b)))

  var rs: ResponseStatus
  var honestVerdict: Option[Report]
  var result: Option[Outcome]

  action init = all {
    rs' = newResponseStatus(CANDS),
    honestVerdict' = None,
    result' = None,
  }

  action honestReport(v: Validator, drawn: Report): bool =
    // Decode, don't constrain: under STRONG the first honest verdict binds the rest.
    val r = if (STRONG) (match honestVerdict { | Some(r0) => r0 | None => drawn }) else drawn
    all {
      result == None,
      HONEST.contains(v),
      canReport(rs, v),
      rs' = addReport(rs, v, r),
      honestVerdict' = (match honestVerdict { | Some(r0) => Some(r0) | None => Some(r) }),
      result' = None,
    }

  action byzReport(v: Validator, r: Report): bool = all {
    result == None,
    BYZ.contains(v),
    canReport(rs, v),
    rs' = addReport(rs, v, r),
    honestVerdict' = honestVerdict,
    result' = None,
  }

  // progress_rotation: resolve once every candidate has reported ...
  action resolveAllReported = all {
    result == None,
    rs.remaining == Set(),
    result' = Some(resolveKeygen(rs)),
    rs' = rs,
    honestVerdict' = honestVerdict,
  }

  // ... or when KeygenResponseTimeout elapses.
  action timeoutResolve = all {
    result == None,
    rs.remaining != Set(),
    HONEST_MAY_TIMEOUT or rs.remaining.intersect(HONEST) == Set(),
    result' = Some(resolveKeygen(rs)),
    rs' = rs,
    honestVerdict' = honestVerdict,
  }

  action step = {
    nondet v = CANDS.oneOf()
    nondet h = HONEST_REPORTS.oneOf()
    nondet k = Set(FRESH_KEY, OTHER_KEY).oneOf()
    nondet blamed = CANDS.powerset().oneOf()
    nondet asSuccess = Set(true, false).oneOf()
    val b = if (asSuccess) SuccessVote(k) else FailureVote(blamed)
    any {
      honestReport(v, h),
      byzReport(v, b),
      resolveAllReported,
      timeoutResolve,
    }
  }

  pure def isFailure(o: Outcome): bool =
    match o { | Failure(_) => true | Success(_) => false }
  pure def offendersOfOutcome(o: Outcome): Set[Validator] =
    match o { | Failure(off) => off | Success(_) => Set() }

  val C1_AcceptanceUnanimity = match result {
    | None => true
    | Some(o) => match o {
        | Success(k) => votersFor(rs, k) == CANDS
        | Failure(_) => true
      }
  }

  val C2_OffendersAreParticipants = match result {
    | None => true
    | Some(o) => offendersOfOutcome(o).subseteq(CANDS)
  }

  // Holds under STRONG with honest nodes reporting in time; fails under the
  // split (NC1, ceremonySplit) and under an outage (ceremonyOutage).
  val C3_HonestNeverOffender = match result {
    | None => true
    | Some(o) => offendersOfOutcome(o).intersect(HONEST) == Set()
  }

  // Every resolved outcome is one the oracle may return.
  val SeamSound = match result {
    | None => true
    | Some(o) => oracleOutcomes(CANDS, FRESH_KEY, BYZ, STRONG and not(HONEST_MAY_TIMEOUT)).contains(o)
  }

  // NC2 target: offenders were non-empty before the drop and empty after it.
  // Must FAIL on ceremonyOutage.
  val NC2_DropNeverEmpties_MustFailHere = match result {
    | None => true
    | Some(o) => not(o == Failure(Set()) and offendersOfOutcome(resolve(rs)) != Set())
  }

  val wResolvedSuccess = result == Some(Success(FRESH_KEY))
  val wResolvedFailure = match result { | None => false | Some(o) => isFailure(o) }
  val wByzantinePunished = match result {
    | None => false
    | Some(o) => offendersOfOutcome(o).intersect(BYZ) != Set()
  }
  val wDropped = match result {
    | None => false
    | Some(o) => o == Failure(Set()) and offendersOfOutcome(resolve(rs)) != Set()
  }
}
```

- [ ] **Step 2: Create `harness.qnt` with the ceremony instances**

```quint
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
```

- [ ] **Step 3: Typecheck, then simulate the strong instance**

Run:
```bash
quint typecheck ceremony.qnt && quint typecheck harness.qnt
quint run harness.qnt --main=ceremonyStrong \
  --invariants C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound \
  --witnesses wResolvedSuccess wResolvedFailure wByzantinePunished \
  --max-steps=6 --max-samples=20000
```
Expected: `[ok] No violation found`; all three witnesses > 0 traces. If `wResolvedSuccess` is 0, the Byzantine draw never picks `SuccessVote(FRESH_KEY)` with everyone else honest: check `HONEST_REPORTS` and that `honestReport` decodes to the first verdict.

- [ ] **Step 4: Run the negative controls (must violate)**

Run:
```bash
quint run harness.qnt --main=ceremonySplit --invariant=NC1_HonestNeverOffender_MustFailHere --max-steps=6 --max-samples=20000
quint run harness.qnt --main=ceremonyOutage --invariant=NC2_DropNeverEmpties_MustFailHere --witnesses wDropped --max-steps=6 --max-samples=20000
```
Expected: both print `[violation]`. The NC1 trace shows honest 1 and 2 on `SuccessVote(7)`, honest 3 on `FailureVote(...)`, Byzantine 4 on `SuccessVote(7)`, and offenders `Set(3)`. The NC2 trace shows two honest non-responders dropped to `Set()`.

- [ ] **Step 5: Exhaustive check of the strong instance**

Run:
```bash
quint verify harness.qnt --main=ceremonyStrong --invariant=C3_HonestNeverOffender --max-steps=6
quint verify harness.qnt --main=ceremonyStrong --invariant=SeamSound --max-steps=6
```
Expected: `[ok]` for both, well under a minute each. Record the times in a scratch note for the README.

- [ ] **Step 6: Update check.sh**

Add `harness.qnt` to the typecheck loop, then append:

```bash
echo "== simulation (ceremony) =="
CEREMONY_STEPS=6
CEREMONY_SAMPLES=20000
CEREMONY_INVARIANTS="C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound"
CEREMONY_WITNESSES="wResolvedSuccess wResolvedFailure wByzantinePunished"
quint run harness.qnt --main=ceremonyStrong --invariants $CEREMONY_INVARIANTS \
  --witnesses $CEREMONY_WITNESSES --max-steps=$CEREMONY_STEPS --max-samples=$CEREMONY_SAMPLES \
  | grep -E '^\[(ok|violation)\]|witnessed in' | sed 's|^|    |'
```

and a `MUST_VIOLATE` section copied from `engine/multisig/quint/check.sh` (the three-outcome loop: `[violation]` passes, `[ok]` is FATAL "control gone inert", no verdict is FATAL "no verdict") with entries:

```bash
MUST_VIOLATE=(
  "harness.qnt:ceremonySplit:NC1_HonestNeverOffender_MustFailHere"
  "harness.qnt:ceremonyOutage:NC2_DropNeverEmpties_MustFailHere"
)
```

Run: `./check.sh` — Expected: typecheck ok ×3, tests pass, simulation `[ok]` with witnesses > 0, both controls `[violation]`.

- [ ] **Step 7: Commit**

```bash
git add state-chain/quint/rotation/ceremony.qnt state-chain/quint/rotation/harness.qnt state-chain/quint/rotation/check.sh
git commit -m "feat: ceremony layer properties, seam and negative controls (PRO-3120)"
```

---

### Task 4: `chain.qnt` — one chain's key rotation status and activation

**Files:**
- Create: `state-chain/quint/rotation/chain.qnt` (module `chain`, pure functions only)
- Modify: `state-chain/quint/rotation/check.sh` (typecheck loop, `quint test chain.qnt`)

**Interfaces:**
- Consumes: `types.*`
- Produces (all used by Task 5): types `Activation`, `Status`, `EpochKey`, `ChainState`, `Outer` (whose handover variant is `KeyHandoverCompleteOuter`), `Async`, `ActivationOutcome`, `ChainStep = { chain, panic: Option[str], logError: Option[str] }`; functions `newChain(spec, genesisKey)`, `chainStatus(c): Async`, `isPending(c)`, `consStatus(chains: Chain -> ChainState): Async`, `startKeygen(c, cands, epoch, freshKey): ChainStep`, `canApplyKeygenOutcome(c)`, `keygenCeremony(c)`, `applyKeygenOutcome(c, o)`, `canApplyVerification(c)`, `verificationParticipants(c)`, `applyVerification(c, isOk, off)`, `startHandover(c, sharing, receiving, epoch): ChainStep`, `canApplyHandoverOutcome(c)`, `handoverCeremony(c)`, `applyHandoverOutcome(c, o)`, `activateKeys(c, newEpoch, outcome): ChainStep`, `progressActivation(c)`, `canGovernanceActivate(c)`, `governanceActivate(c)`, `canSignatureReady(c)`, `signatureReady(c)`, `reset(c)`, `isFailedStatus(c)`, `statusOffenders(c)`, `isComplete(c)`.

Granularity decision (spec "Scheduling"): the side effects the Rust performs inside `status()` on `AwaitingActivationSignatures` are a separate pure step, `progressActivation`, which the validator hook applies to every chain before reading `consStatus`.

- [ ] **Step 1: Write `chain.qnt`**

```quint
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
  // KeyHandoverCompleteOuter: variant tags share one namespace per module, so it
  // cannot reuse Status's KeyHandoverComplete name.
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
```

- [ ] **Step 2: Typecheck and test**

Run: `quint typecheck chain.qnt && quint test chain.qnt`
Expected: 10 passing. The most likely typecheck complaint is a `match` arm returning different record shapes; keep every branch of a `match` returning the same type (`ChainStep` or `ChainState`, never mixed).

- [ ] **Step 3: Update check.sh**

Add `chain.qnt` to the typecheck loop (before `harness.qnt`) and `quint test chain.qnt` under unit tests. Run `./check.sh`.

- [ ] **Step 4: Commit**

```bash
git add state-chain/quint/rotation/chain.qnt state-chain/quint/rotation/check.sh
git commit -m "feat: per-chain key rotation status and activation layer (PRO-3120)"
```

---

### Task 5: `validator.qnt` — the `World`, hooks, environment actions, `step`

**Files:**
- Create: `state-chain/quint/rotation/validator.qnt` (module `rotation`)
- Modify: `state-chain/quint/rotation/harness.qnt` (add instance `main` with `happyPathTest`)
- Modify: `state-chain/quint/rotation/check.sh`

**Interfaces:**
- Consumes: `types.*`, `chain.*`
- Produces: consts `VALIDATORS`, `BYZ`, `CHAINS`, `MIN_SIZE`, `MAX_SIZE`, `CONTRACTION_PERCENT`, `STRONG`, `FAIR`, `K_BLOCKS`; types `RotationState`, `Phase`, `AbortReason`, `History`, `World`; `var w: World`; `init`, `step`; actions `block(txFailed, notRequired, ascending)`, `keygenOutcome(id, succeed, drawnOff)`, `handoverOutcome(id, succeed, drawnOff)`, `verificationResult(id, isOk, drawnOff)`, `signatureReadyOn(id)`, `governanceActivateOn(id)`, `epochElapses`, `toggleSafeMode`, `toggleBroadcastsPending`, `bidderLeaves(v)`, `bidderJoins(v)`, `forceRotation`; pure `candidatesOf(rs)`, `sizeFloor(x)`, `selectSharing(...)`, `isActivatingPhase(p)`, `phaseName(p)`; `val envPending`.

Design notes locked in here:
- One `var w: World`. Every action assigns `w'` once.
- No higher-order helpers: each "apply to all chains" is written out (`resetAll`, `progressAll`, `keygenAll`, `handoverAll`, `activateAll`), so nothing depends on passing operators as values.
- Every `nondet` draw is decoded, never constrained: offender draws are intersected with the allowed set, `FAIR` overrides adverse choices.
- Hook order per block, from `construct_runtime`: validator hook (which first applies `progressActivation` to every chain, the Rust's `status()` side effects), then the session hook.

- [ ] **Step 1: Write `validator.qnt`**

```quint
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
```

- [ ] **Step 2: Typecheck**

Run: `quint typecheck validator.qnt`
Expected: clean. Common fixes: a `match` whose arms return differently-shaped records (make every arm return `World`); `x.phase != Idle` needs `Idle` in scope (it is, from this module); a `val` inside `all { }` must precede the conditions it is used in.

- [ ] **Step 3: Add the `main` instance and the happy-path test to `harness.qnt`**

Append (keep the ceremony instances above it):

```quint
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
```

- [ ] **Step 4: Run the test and a smoke simulation**

Run:
```bash
quint typecheck harness.qnt
quint test harness.qnt --main=main
quint run harness.qnt --main=main --invariant=NoPanicsSmoke --witnesses wEpochAdvanced --max-steps=40 --max-samples=2000
```
Expected: `happyPathTest` passes. The simulation may report `[violation]` of `NoPanicsSmoke`: that is the W5 path (spec section 1) showing up early, and it is a result, not a plan failure. Copy the seed and trace into a scratch note for Task 9. `wEpochAdvanced` must be > 0; if it is 0 at 40 steps, raise `--max-steps` to 80 and check `envPending` and the block guard before suspecting the model.

- [ ] **Step 5: Update check.sh**

Add `validator.qnt` to the typecheck loop (before `harness.qnt`) and `quint test harness.qnt --main=main` under unit tests. Run `./check.sh`.

- [ ] **Step 6: Commit**

```bash
git add state-chain/quint/rotation/validator.qnt state-chain/quint/rotation/harness.qnt state-chain/quint/rotation/check.sh
git commit -m "feat: validator rotation phases, environment and adversary actions (PRO-3120)"
```

---

### Task 6: Properties and witnesses on the `main` instance

**Files:**
- Modify: `state-chain/quint/rotation/validator.qnt` (append the properties section to module `rotation`; delete `wEpochAdvanced` and `NoPanicsSmoke`)
- Modify: `state-chain/quint/rotation/check.sh`

**Interfaces:**
- Consumes: everything from Task 5, plus `SharingRecord.threshold` (recorded by `tryStartKeyHandover`).
- Produces: invariants `R1_BannedNeverAuthority`, `R2_SizeFloor`, `R3_SharingSetValidity`, `R4_TransitionGating`, `R5_NoNextKeyAfterAbort`, `R6_KeyEpochAgreement`, `R7_NoAbortAfterActivation`, `H2_UtxoAlwaysHandsOver`, `H3_NextKeyOnlyAfterActivation`, `H4_ConsSoundness`, `PF_NoPanics`, `NoUnexpectedLogErrors`, `NC3_EveryRetryBans_MustFailHere`, `L1_Termination`, `L2_Progress`; witnesses `W1_FullRotationWithHandover`, `W2_RecoverFromKeygenFailure`, `W3_AbortAtSizeFloor`, `W4_CompleteDespiteSafeMode`, `W5_HandoverVerificationLivelock`, `W6_AbortSharingUnavailable`, `W7_HandoverRetry`.

H1 (verified-implies-unanimous-and-signed) holds by construction of the oracle layer and is covered by `C1` plus `happyPathTest`; it is not a separate invariant.

- [ ] **Step 1: Append the properties to module `rotation`**

```quint
  // ---------- properties (identifiers from DESIGN.md) ----------

  pure def rsOf(p: Phase): Option[RotationState] =
    match p {
    | KeygensInProgress(rs) => Some(rs)
    | KeyHandoversInProgress(rs) => Some(rs)
    | ActivatingKeys(rs) => Some(rs)
    | NewKeysActivated(rs) => Some(rs)
    | _ => None
    }

  val activeChains: Set[Chain] = CHAIN_IDS.filter(id => w.chains.get(id).spec.vault == Active)

  // R1: candidates never overlap the banned set (try_restart_keygen's
  // debug_assert), and the queued and installed sets exclude everyone banned
  // in the rotation that produced them.
  val R1_BannedNeverAuthority = and {
    match rsOf(w.phase) { | Some(rs) => rs.primary.intersect(rs.banned) == Set() | None => true },
    match w.phase {
    | SessionRotating(auths) =>
        match w.hist.lastRs { | Some(rs) => auths.intersect(rs.banned) == Set() | None => false }
    | _ => true
    },
    (w.phase == Idle) implies
      (match w.hist.lastRs { | Some(rs) => w.authorities.intersect(rs.banned) == Set() | None => true }),
  }

  // R2: the queued set respects max(min_size, contraction floor of the OLD set).
  val R2_SizeFloor = match w.phase {
    | SessionRotating(auths) => auths.size() >= sizeFloor(w)
    | _ => true
  }

  // R3: sharing participants are unbanned current authorities, at least the
  // success threshold of the set at the time (recorded with the sharing set).
  val R3_SharingSetValidity = match w.hist.lastSharing {
    | None => true
    | Some(s) => and {
        s.sharing.subseteq(s.current),
        s.current.intersect(s.banned) == Set(),
        s.sharing.size() >= s.threshold,
      }
  }

  // R4: the epoch never advances unless every chain is Complete, and the
  // queued authorities are the final candidate set.
  val R4_TransitionGating = match w.phase {
    | SessionRotating(auths) => and {
        CHAIN_IDS.forall(id => isComplete(w.chains.get(id))),
        match w.hist.lastRs { | Some(rs) => auths == candidatesOf(rs) | None => false },
      }
    | _ => true
  }

  // R5: in Idle no active chain holds a key for a future epoch.
  val R5_NoNextKeyAfterAbort = (w.phase == Idle) implies activeChains.forall(id =>
    match w.chains.get(id).activeKey { | Some(ek) => ek.epoch <= w.epoch | None => true })

  // R6: active chains agree on the current key epoch.
  val R6_KeyEpochAgreement = activeChains.forall(a => activeChains.forall(b =>
    match w.chains.get(a).activeKey {
    | Some(ka) => match w.chains.get(b).activeKey { | Some(kb) => ka.epoch == kb.epoch | None => true }
    | None => true
    }))

  // R7: no abort edge exists past ActivatingKeys (L3's safety half).
  val R7_NoAbortAfterActivation =
    w.hist.abortedFrom.intersect(Set("ActivatingKeys", "NewKeysActivated", "SessionRotating")) == Set()

  // H2: a UTXO chain that has a key never reaches KeyHandoverComplete without
  // running the handover ceremony. Assumes UTXO chains start with a key.
  val H2_UtxoAlwaysHandsOver = CHAIN_IDS.forall(id =>
    val c = w.chains.get(id)
    (c.spec.kind == Utxo and c.activeKey != None
      and (match c.status { | KeyHandoverComplete(_) => true | _ => false }))
      implies c.handoverRan)

  // H3: the next-epoch key exists only during activation and the session steps.
  val H3_NextKeyOnlyAfterActivation = CHAIN_IDS.forall(id =>
    match w.chains.get(id).activeKey {
    | Some(ek) => ek.epoch <= w.epoch + 1 and ((ek.epoch == w.epoch + 1) implies isActivatingPhase(w.phase))
    | None => true
    })

  // H4: a merged Failed comes from a failed chain and carries exactly their
  // offenders. May fail by design for mixed Ready pairs (DESIGN.md).
  val H4_ConsSoundness = match consStatus(w.chains) {
    | Ready(o) => match o {
        | FailedOuter(off) => and {
            CHAIN_IDS.exists(id => isFailedStatus(w.chains.get(id))),
            off == CHAIN_IDS.map(id => statusOffenders(w.chains.get(id))).flatten(),
          }
        | _ => true
      }
    | _ => true
  }

  // PF1-PF4: no assertion site in key_rotator.rs is reachable.
  val PF_NoPanics = w.panics == Set()
  // The log::error! sites on_initialize and activate_keys treat as impossible.
  val NoUnexpectedLogErrors = w.logErrors == Set()

  // NC3: every keygen restart or handover retry grows the banned set. Expected to FAIL.
  val NC3_EveryRetryBans_MustFailHere = not(w.hist.retryWithoutBan)

  // Liveness, as bounded invariants. Meaningful on the fair instance only.
  val L1_Termination = (w.phase != Idle) implies (w.blocks - w.hist.rotationStartedAt <= K_BLOCKS)
  val L2_Progress = w.hist.aborts == 0

  // ---------- witnesses (must be reachable) ----------

  val W1_FullRotationWithHandover =
    w.hist.epochsAdvanced >= 1 and CHAIN_IDS.exists(id => w.chains.get(id).handoverRan)
  val W2_RecoverFromKeygenFailure = w.hist.epochsAdvanced >= 1 and w.hist.keygenRestarts >= 1
  val W3_AbortAtSizeFloor = w.hist.lastAbortReason == Some(NotEnoughCandidates)
  val W4_CompleteDespiteSafeMode = w.hist.epochsAdvanced >= 1 and w.hist.safeModeOffWhileActivating
  val W5_HandoverVerificationLivelock = and {
    match w.phase { | KeyHandoversInProgress(_) => true | _ => false },
    CHAIN_IDS.exists(id => w.chains.get(id).status == Failed(Set())),
    w.panics.contains("handover_invalid_state"),
  }
  val W6_AbortSharingUnavailable = w.hist.lastAbortReason == Some(SharingUnavailable)
  val W7_HandoverRetry = w.hist.handoverRetries >= 1
```

- [ ] **Step 2: Typecheck and simulate every safety invariant with every witness**

Run:
```bash
quint typecheck validator.qnt && quint typecheck harness.qnt
quint run harness.qnt --main=main \
  --invariants R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating \
    R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation \
    H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation H4_ConsSoundness NoUnexpectedLogErrors \
  --witnesses W1_FullRotationWithHandover W2_RecoverFromKeygenFailure W3_AbortAtSizeFloor \
    W4_CompleteDespiteSafeMode W5_HandoverVerificationLivelock W6_AbortSharingUnavailable W7_HandoverRetry \
  --max-steps=40 --max-samples=20000
```
Expected: `[ok]` for the batch above (PF_NoPanics is run separately, next step). Witness counts: W1, W2, W3, W4, W7 must be > 0. W5 and W6 are results either way; record the counts. If W2 is 0, the Byzantine keygen failure is not being attributed: check that `decodeOutcome` keeps `Failure(Set(4))` when `drawnOff` contains 4 and `STRONG` is true. If W3 is 0, remember the floor is 3 at n=4: it needs the Byzantine banned, then a second offender, which under STRONG requires the split instance or a non-responder path, so W3 is expected to read 0 on `main` and > 0 on `split` (Task 7); record that.

If any invariant is violated, re-run with the printed seed and `--verbosity=3 --mbt`, save the trace as `findings/<Id>-<seed>.txt` in a scratch directory (not committed yet), and continue; Task 9 triages.

- [ ] **Step 3: Simulate the panic-freedom invariant and the NC3 control separately**

Run:
```bash
quint run harness.qnt --main=main --invariant=PF_NoPanics --witnesses W5_HandoverVerificationLivelock --max-steps=40 --max-samples=20000
quint run harness.qnt --main=main --invariant=NC3_EveryRetryBans_MustFailHere --max-steps=40 --max-samples=20000
```
Expected: NC3 prints `[violation]` (an unattributed failure retried with an unchanged banned set). PF_NoPanics prints `[violation]` if W5 is reachable; save the seed and trace for Task 9. If PF_NoPanics is `[ok]` and W5 reads 0, that is also a result: record it.

- [ ] **Step 4: Exhaustive verification on `main`**

Run each, noting wall-clock time:
```bash
for inv in R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating \
           R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation \
           H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation H4_ConsSoundness NoUnexpectedLogErrors; do
  /usr/bin/time -p quint verify harness.qnt --main=main --invariant=$inv --max-steps=25 2>&1 | grep -E '^\[|^real'
done
```
Expected: `[ok]` for each within a few minutes. If one exceeds ~5 minutes, lower `--max-steps` to 20 for that property and record the bound in the README; if it still does not fit, apply depth compression (spec "Scheduling") as a separate follow-up task rather than silently shrinking the instance. A `[violation]` here that simulation missed is a finding: save the ITF trace with `--out-itf`.

- [ ] **Step 5: Update check.sh**

Replace the smoke lines with the full `main` simulation from Step 2 (as a `MAIN_INVARIANTS` / `MAIN_WITNESSES` pair), a separate `PF_NoPanics` run whose result is printed but does not fail the script (its verdict is a finding, recorded in the README), and add to `MUST_VIOLATE`:

```bash
  "harness.qnt:main:NC3_EveryRetryBans_MustFailHere"
```

Run `./check.sh`.

- [ ] **Step 6: Commit**

```bash
git add state-chain/quint/rotation/validator.qnt state-chain/quint/rotation/check.sh
git commit -m "feat: rotation safety, panic-freedom and control properties (PRO-3120)"
```

---

### Task 7: `uninit`, `split` and `fair` instances; bounded liveness

**Files:**
- Modify: `state-chain/quint/rotation/harness.qnt`
- Modify: `state-chain/quint/rotation/check.sh`

**Interfaces:**
- Consumes: module `rotation` and its properties.
- Produces: instances `uninit`, `split`, `fair`; witness `W8_HonestBannedUnderSplit` (in `split`); run test `fairRotationCompletesTest` (in `fair`).

- [ ] **Step 1: Append the instances to `harness.qnt`**

```quint
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
```

- [ ] **Step 2: Run the fair test and the bounded liveness checks**

Run:
```bash
quint typecheck harness.qnt
quint test harness.qnt --main=fair
quint run harness.qnt --main=fair --invariants L1_Termination L2_Progress R7_NoAbortAfterActivation PF_NoPanics \
  --witnesses W1_FullRotationWithHandover W2_RecoverFromKeygenFailure --max-steps=60 --max-samples=20000
```
Expected: test passes; `[ok]`; W1 and W2 > 0. If `L1_Termination` is violated, read the trace: either a block passed while a delivery was pending (the `block` guard or `envPending` is incomplete: every `can…` predicate must be listed) or the rotation genuinely needs more than `K_BLOCKS` (raise it and record why). If `L2_Progress` is violated, the fair environment still allows an abort: check `decodeOutcome` under FAIR and that `toggleSafeMode`/`toggleBroadcastsPending`/`bidderLeaves` are disabled by `not(FAIR)`.

- [ ] **Step 3: Run `uninit` and `split`**

Run:
```bash
quint run harness.qnt --main=uninit \
  --invariants R4_TransitionGating R5_NoNextKeyAfterAbort R6_KeyEpochAgreement H3_NextKeyOnlyAfterActivation NoUnexpectedLogErrors \
  --witnesses W9_UninitialisedChainCompletesWithoutKey --max-steps=40 --max-samples=20000
quint run harness.qnt --main=split \
  --invariants R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation \
  --witnesses W3_AbortAtSizeFloor W8_HonestBannedUnderSplit --max-steps=40 --max-samples=20000
```
Expected: `[ok]` on both; W9 > 0; W8 > 0; W3 > 0 on `split` (two bans reach the floor of 3 from 4).

- [ ] **Step 4: Exhaustive liveness on `fair`, and the optional temporal attempt**

Run:
```bash
quint verify harness.qnt --main=fair --invariant=L1_Termination --max-steps=30
quint verify harness.qnt --main=fair --invariant=L2_Progress --max-steps=30
```
Expected: `[ok]`. Optional, time-boxed to 20 minutes: `quint verify harness.qnt --main=main --temporal=EventuallyIdle --max-steps=25` after adding `temporal EventuallyIdle = always(eventually(w.phase == Idle))` to module `rotation`. Record the result in the README whichever way it goes: a counterexample is the NC3 loop (expected); a timeout is recorded as "temporal mode does not fit at depth 25".

- [ ] **Step 5: Update check.sh**

Add `quint test harness.qnt --main=fair` under unit tests; add `uninit`, `split`, `fair` simulation blocks mirroring Steps 2 and 3 (invariants and required witnesses as listed); add `L1_Termination` and `L2_Progress` on `fair` to the `--verify` section.

Run `./check.sh`.

- [ ] **Step 6: Commit**

```bash
git add state-chain/quint/rotation/harness.qnt state-chain/quint/rotation/check.sh
git commit -m "feat: uninitialised, split and fair rotation instances with bounded liveness (PRO-3120)"
```

---

### Task 7b: Apalache tractability for the validator layer (time-boxed)

**Why this task exists.** Task 6 found that `quint verify` on `main` is intractable as encoded: depth 1 takes about 72 s, depth 2 ran over two hours with no verdict, depth 8 exhausts Apalache's default 4 GB heap. The spec's first two levers (depth compression, fewer chains) do not address a per-transition encoding that is already too large at depth 2. This task tries the levers below in order, stops at the first that reaches depth 10 within 10 minutes per property, and records the outcome either way. Budget: at most three hours of wall-clock runs. Nothing here may weaken a property or shrink the reachable set of an instance that other tasks depend on.

**Files:**
- Modify: `state-chain/quint/rotation/validator.qnt` (lever A only, if adopted)
- Modify: `state-chain/quint/rotation/harness.qnt` (lever B instance)
- Create: `.superpowers/sdd/PLAN/findings/tractability.md` (scratch, not committed; Task 8 copies the table into the README)

**Interfaces:**
- Consumes: module `rotation` as of Task 7.
- Produces: a table `lever | property | depth | verdict | seconds | notes`, and whichever code change (if any) made verification tractable, with tests and witnesses still passing.

- [ ] **Step 1: Baseline with Apalache tuning only (lever C)**

For `R2_SizeFloor` on `main`, run each of these with a 10-minute cap (`timeout 600`), recording verdict and time:

```bash
cd state-chain/quint/rotation
JVM_ARGS="-Xmx8g" timeout 600 quint verify harness.qnt --main=main --invariant=R2_SizeFloor --max-steps=2
JVM_ARGS="-Xmx8g" timeout 600 quint verify harness.qnt --main=main --invariant=R2_SizeFloor --max-steps=2 \
  --apalache-config='{"checker":{"smt-encoding":"funArrays"}}'
JVM_ARGS="-Xmx8g" timeout 600 quint verify harness.qnt --main=main --invariant=R2_SizeFloor --max-steps=2 \
  --apalache-config='{"checker":{"smt-encoding":"arrays","tuning":{"search.invariant.mode":"after"}}}'
```

If any finishes, climb the depth ladder 4, 6, 10 with the same flags. Record the first configuration that reaches depth 10 under 10 minutes; if none, continue.

- [ ] **Step 2: One-chain instance (lever B)**

Append to `harness.qnt`:

```quint
// One UTXO chain only: the smallest instance that still exercises keygen,
// handover, activation and the session steps. Used for exhaustive checks
// when the two-chain instance does not fit.
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
```

Run the same ladder (depths 2, 4, 6, 10, cap 10 minutes, best flags from Step 1) for `R2_SizeFloor`, then for `R4_TransitionGating` and `H3_NextKeyOnlyAfterActivation`. Record. Note that `H4_ConsSoundness` is trivial on one chain and must not be reported as verified from `main1`.

- [ ] **Step 3: Per-phase block transitions (lever A), only if Steps 1–2 did not reach depth 10 on `main`**

Refactor `rotation` so Apalache sees five small transitions instead of one large one, without changing the reachable set:

1. Split `validatorHook` into `hookIdle(x)`, `hookKeygen(x, rs, ascending)`, `hookHandover(x, rs, txFailed, notRequired, ascending)`, `hookActivating(x, rs)`, each containing exactly the corresponding `match` arm's body from the current `validatorHook` (after `progressAll` and `consStatus`, which each one computes itself). Keep `validatorHook` as a thin dispatcher over the phase so `blockStep` and the existing tests are unchanged.
2. Replace the `block` action's body with:

```quint
  action block(txFailed: Set[Chain], notRequired: Set[Chain], ascending: bool): bool = all {
    not(FAIR and envPending),
    any {
      all { w.phase == Idle, w' = finishBlock(sessionHook(hookIdle(progressAll(w)))) },
      all { isKeygenPhase(w.phase), w' = finishBlock(sessionHook(hookKeygen(progressAll(w), rsOfOrEmpty(w.phase), ascending))) },
      all { isHandoverPhase(w.phase), w' = finishBlock(sessionHook(hookHandover(progressAll(w), rsOfOrEmpty(w.phase), if (FAIR) Set() else txFailed, notRequired, ascending))) },
      all { isActivatingKeysPhase(w.phase), w' = finishBlock(sessionHook(hookActivating(progressAll(w), rsOfOrEmpty(w.phase)))) },
      all { isSessionPhase(w.phase), w' = finishBlock(sessionHook(w)) },
    },
  }
```

with `finishBlock(x)` = the block-counter and `safeModeOffWhileActivating` bookkeeping currently at the end of `blockStep`, `rsOfOrEmpty(p)` = the `RotationState` carried by `p` or `{ primary: Set(), banned: Set(), newEpoch: 0 }`, and the four phase predicates as `pure def`s. The guards are mutually exclusive and exhaustive over `Phase`, so exactly one arm is enabled in every state, which is what makes this equivalent to the old `block`.
3. `happyPathTest`, `fairRotationCompletesTest`, and every simulation in `check.sh` must still pass with the same verdicts; re-run `./check.sh` and record the witness counts before and after (they should move only by sampling noise).
4. Re-run the ladder on `main` with the best flags. Record.

- [ ] **Step 4: Symbolic simulation as a last resort (lever D)**

If depth 10 is still out of reach on `main`, run `quint verify --random-transitions --max-steps=25` for each safety invariant on `main` with a 10-minute cap, and record the verdicts explicitly labelled "symbolic simulation, not exhaustive".

- [ ] **Step 5: Record and commit**

Write the table to the scratch file with one paragraph of conclusion: which instance and depth are exhaustively verified, which lever did it, what remains simulation-only. If lever A was adopted, commit `validator.qnt` (and `harness.qnt` if `main1` was added) as `refactor: per-phase block transitions for Apalache tractability (PRO-3120)`; otherwise commit only `harness.qnt` (`main1`) as `feat: one-chain instance for exhaustive checks (PRO-3120)`. Task 8 puts the table in the README and wires the tractable verify runs into `check.sh --verify`.

---

### Task 8: `check.sh` complete, README status tables

**Files:**
- Modify: `state-chain/quint/rotation/check.sh`
- Modify: `state-chain/quint/rotation/README.md`
- Modify: `state-chain/quint/rotation/harness.qnt` (comment fix only: `fairRotationCompletesTest` says "Nine blocks"; the run has six)
- Create: `state-chain/quint/rotation/apalache-no-deadlocks.json` containing `{"checker":{"no-deadlocks":true}}`

**Interfaces:**
- Consumes: every instance, invariant, witness and control named in Tasks 3, 6, 7 and 7b, and the tractability table in `.superpowers/sdd/PLAN/findings/tractability.md`.

**Reconcile with what Tasks 6–7 already put in `check.sh`.** The script already has `MAIN_STEPS=80`/`MAIN_SAMPLES=20000`, a `main` block, a non-enforced `PF_NoPanics` run, a `run_instance` helper (from Task 7) that fails the script when a required witness reads zero, blocks for `uninit`/`split`/`fair` (`fair` at 2000 samples with a comment), and a `--verify` section holding the ceremony invariants and depth-1 `fair` entries. Task 8 does not rewrite it from the sketch below; it (1) routes the `main` and `ceremonyStrong` simulations through the same required-witness helper so a dead `W1`/`W2`/`W4`/`W7` or ceremony witness fails the script, (2) rebuilds the `--verify` section as specified in Step 1, and (3) keeps every existing comment that explains a deviation.

**Apalache facts from Task 7b that override the sketch below.** `--apalache-config` takes a file path, not inline JSON (Task 3 already used a file, `_apalache_nodeadlock.json`, and its depth-6 ceremony verdicts stand; the file created here just makes that config a committed artifact, and the four ceremony verifies are re-run because they cost about 6 s each); `checker.smt-encoding` is written `{"type":"fun-arrays"}`; the JVM heap is `JVM_ARGS=-Xmx8g`; `quint verify` leaves a server on port 8822, so verify runs are sequential; killing `quint` orphans `quint_evaluator` (kill it explicitly). The validator layer is exhaustively verifiable only at depth 1 on `main` and depth 2 on `main1` (three properties, ~140–250 s each with fun-arrays and 8 GiB), neither of which reaches a completed rotation; the README must say so plainly and present simulation as the evidence for that layer.
- Produces: a `./check.sh` that exits non-zero on any typecheck failure, test failure, invariant violation outside the recorded-findings set, required witness at zero, or inert negative control; `./check.sh --verify` adds Apalache.

- [ ] **Step 1: Finalise check.sh**

Structure, in order (copy the three-outcome `MUST_VIOLATE` loop from `engine/multisig/quint/check.sh` verbatim):

```bash
#!/usr/bin/env bash
# Run every Quint check for the rotation model.
#   ./check.sh          typecheck, tests, simulation, negative controls (~1-2 min)
#   ./check.sh --verify add exhaustive Apalache checks (budget ~15 min)
set -euo pipefail
cd "$(dirname "$0")"
command -v quint >/dev/null || { echo "quint not on PATH; see README.md"; exit 1; }

STEPS=40
SAMPLES=20000
VERIFY_STEPS=25

echo "== typecheck =="
for f in types.qnt ceremony.qnt chain.qnt validator.qnt harness.qnt; do
  if quint typecheck "$f"; then echo "  ok $f"; else echo "  FAILED $f"; exit 1; fi
done

echo "== unit tests =="
quint test types.qnt
quint test ceremony.qnt --main=ceremony
quint test chain.qnt
quint test harness.qnt --main=main
quint test harness.qnt --main=fair

# Required-positive witnesses: a zero count makes the surrounding run vacuous.
# W5 and W6 are NOT required-positive; their counts are findings (README).
require_witnesses() {
  # $1 = quint output, $2.. = witness names that must be > 0
  local out="$1"; shift
  for wname in "$@"; do
    if printf '%s\n' "$out" | grep -E "^${wname} was witnessed in 0 trace" >/dev/null; then
      echo "FATAL: required witness ${wname} was never reached." >&2; exit 1
    fi
  done
}

run_sim() {
  # $1 main, $2 invariants (space separated), $3 witnesses (space separated), $4.. required witnesses
  local main="$1" invs="$2" wits="$3"; shift 3
  echo "  main=${main}"
  local out
  out=$(quint run harness.qnt --main="$main" --invariants $invs --witnesses $wits \
        --max-steps=$STEPS --max-samples=$SAMPLES 2>&1)
  printf '%s\n' "$out" | grep -E '^\[(ok|violation)\]|witnessed in' | sed 's|^|    |'
  printf '%s\n' "$out" | grep -q '^\[ok\]' || { echo "FATAL: invariant violated on ${main}" >&2; exit 1; }
  require_witnesses "$out" "$@"
}

echo "== simulation (ceremony) =="
run_sim ceremonyStrong \
  "C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound" \
  "wResolvedSuccess wResolvedFailure wByzantinePunished" \
  wResolvedSuccess wResolvedFailure wByzantinePunished

echo "== simulation (main) =="
run_sim main \
  "R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation H4_ConsSoundness NoUnexpectedLogErrors" \
  "W1_FullRotationWithHandover W2_RecoverFromKeygenFailure W4_CompleteDespiteSafeMode W5_HandoverVerificationLivelock W6_AbortSharingUnavailable W7_HandoverRetry" \
  W1_FullRotationWithHandover W2_RecoverFromKeygenFailure W4_CompleteDespiteSafeMode W7_HandoverRetry

echo "== simulation (uninit) =="
run_sim uninit \
  "R4_TransitionGating R5_NoNextKeyAfterAbort R6_KeyEpochAgreement H3_NextKeyOnlyAfterActivation NoUnexpectedLogErrors" \
  "W9_UninitialisedChainCompletesWithoutKey" W9_UninitialisedChainCompletesWithoutKey

echo "== simulation (split) =="
run_sim split \
  "R1_BannedNeverAuthority R2_SizeFloor R3_SharingSetValidity R4_TransitionGating R5_NoNextKeyAfterAbort R6_KeyEpochAgreement R7_NoAbortAfterActivation H2_UtxoAlwaysHandsOver H3_NextKeyOnlyAfterActivation" \
  "W3_AbortAtSizeFloor W8_HonestBannedUnderSplit" W3_AbortAtSizeFloor W8_HonestBannedUnderSplit

echo "== simulation (fair, bounded liveness) =="
run_sim fair "L1_Termination L2_Progress R7_NoAbortAfterActivation PF_NoPanics" \
  "W1_FullRotationWithHandover W2_RecoverFromKeygenFailure" W1_FullRotationWithHandover W2_RecoverFromKeygenFailure

# PF_NoPanics on main is reported, not enforced: its verdict is the W5 finding
# (see README "Findings"). If it ever flips, the README must be updated.
echo "== panic freedom (main; reported) =="
quint run harness.qnt --main=main --invariant=PF_NoPanics --witnesses W5_HandoverVerificationLivelock \
  --max-steps=$STEPS --max-samples=$SAMPLES 2>&1 | grep -E '^\[(ok|violation)\]|witnessed in' | sed 's|^|    |' || true

MUST_VIOLATE=(
  "harness.qnt:ceremonySplit:NC1_HonestNeverOffender_MustFailHere"
  "harness.qnt:ceremonyOutage:NC2_DropNeverEmpties_MustFailHere"
  "harness.qnt:main:NC3_EveryRetryBans_MustFailHere"
)
# ... three-outcome loop from engine/multisig/quint/check.sh, unchanged ...

if [[ "${1:-}" == "--verify" ]]; then
  echo "== exhaustive verification (slow, ~13 min; runs are sequential: Apalache holds port 8822) =="
  # The ceremony layer is a one-shot model: without no-deadlocks Apalache reports
  # a deadlock once the ceremony resolves. Config is a FILE path.
  verify() { # main invariant depth [extra flags...]
    local main="$1" inv="$2" depth="$3"; shift 3
    echo "  ${main}::${inv} depth ${depth} ..."
    JVM_ARGS="-Xmx8g" timeout 900 quint verify harness.qnt --main="$main" --invariant="$inv" --max-steps="$depth" "$@" \
      | grep -E '^\[(ok|violation)\]' | sed "s|^|  ${main}::${inv} |" || echo "  ${main}::${inv} NO VERDICT (timeout or error)"
  }
  for inv in C1_AcceptanceUnanimity C2_OffendersAreParticipants C3_HonestNeverOffender SeamSound; do
    verify ceremonyStrong $inv 6 --apalache-config=apalache-no-deadlocks.json
  done
  # Validator layer: only these fit. Depth 1 on the two-chain instance and depth 2
  # on the one-chain instance; neither reaches a completed rotation (README "Status").
  for inv in L1_Termination L2_Progress; do verify fair $inv 1; done
  for inv in R2_SizeFloor R4_TransitionGating H3_NextKeyOnlyAfterActivation; do
    verify main1 $inv 2 --apalache-config=apalache-fun-arrays.json
  done
fi
```

Where a Task 6/7 result changed a bound (a property only fits at depth 20, say), encode that bound here per property rather than lowering `VERIFY_STEPS` for everything.

- [ ] **Step 2: Run both modes and record timings**

Run: `./check.sh` then `time ./check.sh --verify`. Expected: simulation mode exits 0; verify mode within ~15 minutes. Keep the verify output for the README.

- [ ] **Step 3: Fill in README "Status", "Known gaps"**

Status: one table per layer with columns Property | Meaning | Simulated (instance, depth, samples) | Verified (instance, depth, time, or "no"). A witness-coverage table with counts from the last `./check.sh`. A "Negative controls" table with the three controls and their observed `[violation]` traces in one line each. A "Tractability" subsection copying the table from `.superpowers/sdd/PLAN/findings/tractability.md` and stating in one sentence that the validator layer's exhaustive coverage (depth 1 on `main`, depth 2 on `main1`) does not reach a completed rotation, so the evidence for that layer is simulation at depth 80. A "Not verified / deferred" list: temporal mode (not attempted, intractable), n=7, multi-vault, per-phase block transitions (tried, worse, reverted).

Also create `apalache-fun-arrays.json` containing `{"checker":{"smt-encoding":{"type":"fun-arrays"}}}` next to the no-deadlocks file, and fix the "Nine blocks" comment in `harness.qnt` to "Six blocks".

Known gaps, copied from DESIGN.md "Known gaps" plus anything discovered (e.g. "UTXO chains are assumed to hold a genesis key; the bootstrap-without-key path is exercised only by `utxoWithoutKeySkipsHandoverTest`").

Add a source-file correspondence map:

```text
ceremony.qnt  <-> state-chain/pallets/cf-threshold-signature/src/response_status.rs, lib.rs (progress_rotation)
chain.qnt     <-> state-chain/pallets/cf-threshold-signature/src/key_rotator.rs, lib.rs (on_key_verification_result, terminate_rotation),
                  state-chain/pallets/cf-vaults/src/vault_activator.rs, state-chain/runtime/src/chainflip/cons_key_rotator.rs
validator.qnt <-> state-chain/pallets/cf-validator/src/lib.rs (on_initialize, rotation helpers, session hooks), helpers.rs
```

- [ ] **Step 4: Commit**

```bash
git add state-chain/quint/rotation/check.sh state-chain/quint/rotation/README.md state-chain/quint/rotation/harness.qnt state-chain/quint/rotation/apalache-no-deadlocks.json state-chain/quint/rotation/apalache-fun-arrays.json
git commit -m "chore: complete rotation model check script and status (PRO-3120)"
```

---

### Task 9: Findings triage

**Files:**
- Modify: `state-chain/quint/rotation/README.md` (section "Findings")
- Create (scratch, not committed): one trace file per finding
- Linear: one issue per surviving finding, under Formal Protocol Verification, related to PRO-3120

**Interfaces:**
- Consumes: every `[violation]`, every zero-count witness that was expected positive, and the W5/W6 counts from Tasks 6–7.

- [ ] **Step 1: Reproduce each candidate finding deterministically**

For each violated invariant or reached must-reach witness on `main`/`uninit`/`split` (not the NC controls, not `fair`):

```bash
quint run harness.qnt --main=<instance> --invariant=<Id> --seed=<seed> --verbosity=3 --mbt --max-steps=40 --out-itf=finding_<Id>.itf.json
```

Then write a deterministic `run <Id>ReproTest` in the relevant harness module that drives the same action sequence with concrete arguments and `.expect(not(<Id>))` (for a violation) or `.expect(<W>)` (for a witness). It must pass under `quint test`. This is the model-side regression guard; it stays in `harness.qnt`.

- [ ] **Step 2: Classify against the Rust**

For each reproduced trace, walk the Rust path it names (the transition comments give the function names) and decide one of:
- **Real**: the Rust does what the model says and the consequence matters (livelock, panic, wrong set). Write it up.
- **Modelling gap**: the Rust has a guard the model lacks. Fix the model, re-run Tasks 6–7 for the affected instance, and note the gap in the README under "Corrections during triage".
- **By design**: the behaviour is intended (e.g. NC3's unbounded retry). Record it in the README with the rationale and leave the control in place.

- [ ] **Step 3: Record and file**

README "Findings": one entry per Real/By-design item: identifier, one-paragraph description, the reproducing test name, classification, Linear link. For each Real item create a Linear issue (team Protocol, project Formal Protocol Verification, related to PRO-3120) titled `Rotation model finding: <short name>`, body = the README entry plus the trace summary and the Rust path, and a note that a `cf-integration-tests/src/authorities.rs` regression test is the next step (separate ticket, since this branch changes no Rust).

Expected candidates going in, to be confirmed or refuted by the runs:
- W5 handover-verification livelock (`PF_NoPanics` on `main`).
- H4 mixed-Ready `Failed(∅)` from the combinator, if a mixed pair is reachable.
- NC3 unbounded retry without bans (by design; document).
- `forceRotation` bypassing the broadcasts-pending gate (by design or finding; the model allows it faithfully, add a witness `W10_ForcedRotationWhileBroadcastsPending` if a trace is wanted).

- [ ] **Step 4: Update PRO-3120 and commit**

Add a comment on PRO-3120 summarising: verified property list with depths and times, control results, findings with links. Then:

```bash
git add state-chain/quint/rotation/README.md state-chain/quint/rotation/harness.qnt
git commit -m "doc: rotation model findings and repro tests (PRO-3120)"
```

---

## Plan self-review notes

- **Spec coverage.** Boundary B: Tasks 4–5. Oracle contract and seam: Tasks 1 (`oracleOutcomes`), 3 (`SeamSound`). C1–C3, NC1, NC2: Task 3. H2–H4, PF1–PF4: Tasks 4 (sites) and 6 (invariants). R1–R7: Task 6. NC3: Task 6. L1–L3: Task 7 (`L1_Termination`, `L2_Progress`, `R7_NoAbortAfterActivation`). W1–W6: Tasks 6–7 (plus W7–W9). Instances `main`/`uninit`/`split`/`fair`: Tasks 5 and 7. `check.sh` and README: Tasks 1, 3, 8. Findings and Linear: Task 9. H1 is by construction and covered by C1 plus `happyPathTest`; the spec's H1 row should be read that way.
- **Deviations from DESIGN.md, applied to the spec in the same commit as this plan:** the 70% floor at n=4 is 3 (nearest rounding), so W3 needs two bans and is expected on `split`, not `main`; NC2 needs the `ceremonyOutage` instance because at n=4/f=1 with honest nodes reporting in time the offender set never reaches the failure threshold; the handover key-mismatch branch is subsumed by an unattributed failure under the oracle and is exercised only at the ceremony layer (`handoverKeyMismatchIsUnattributedTest`).
- **Type consistency checked:** `SharingRecord` carries `threshold` (Task 5) and R3 reads it (Task 6); `Status` variants carry `participants` for verification (Task 4) and `verificationResult` intersects with them (Task 5); `ChainStep.panic`/`logError` are `Option[str]` and `collect` folds them with `somes`; `decodeOutcome` returns `types.Outcome`, which `applyKeygenOutcome`/`applyHandoverOutcome` consume.
