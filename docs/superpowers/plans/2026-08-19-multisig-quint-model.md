# Multisig Quint Model Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Quint model of the multisig keygen ceremony that exhaustively checks Byzantine attribution correctness — above all, that an honest node is never blamed.

**Architecture:** A concrete lemma layer (`broadcast.qnt`) models both broadcast primitives and proves their postconditions. An idealised oracle (`oracle.qnt`) restates those postconditions as a contract. The ten-stage keygen machine (`keygen.qnt`) is built on the oracle, which is what keeps it checkable. Cryptography is abstract throughout.

**Tech Stack:** Quint 0.32.0 (npm), Apalache 0.56.1 (auto-downloaded, needs a JDK), the Quint Rust evaluator 0.6.0 (auto-downloaded).

**Spec:** `docs/superpowers/specs/2026-08-19-frost-quint-model-design.md`

## Global Constraints

These were established by a working prototype. Violating any of them means rewriting modules later.

- **Never use `setOfMaps(D, C).oneOf()`.** Apalache rejects it: *"Trying to expand a set of functions. This will blow up the solver."* `quint run` accepts it, so the failure only appears at `quint verify` time. Encode every nondeterministic choice as `Set[record].powerset().oneOf()` plus explicit well-formedness constraints.
- **`Option` is not in the Quint standard library.** Define local sum types instead.
- **`quint eval` does not exist in 0.32.0.** To evaluate an expression: `quint repl -r FILE.qnt::MODULE` with the expression on stdin.
- **`quint typecheck` prints nothing on success.** Assert on exit code, not output.
- **PATH:** under nvm the binary lands in `$(dirname $(which node))/quint` and may not be on a non-interactive shell's PATH. Every command below assumes `quint` resolves; if it does not, prefix with `export PATH="$(dirname $(readlink -f $(which node))):$PATH"`.
- **Cross-file imports** use `import types.* from "./types"` (no `.qnt` extension).
- **Tuple-keyed map types need double parentheses:** `((Party, Party)) -> Vote`.
- All models live in `engine/multisig/quint/`. No Rust is modified by this plan.
- Quorum is `count > threshold` where `threshold(n) = (2n − 1) / 3` in integer arithmetic — copy this exactly; it is `threshold_from_share_count` in `utilities/src/lib.rs:72`.

## File Structure

| File | Responsibility |
| --- | --- |
| `engine/multisig/quint/README.md` | install, run, measured timings, PATH gotcha |
| `engine/multisig/quint/types.qnt` | `Party`, `Value`, `Vote`, `Send`, `Claim`, `Opt`, `Outcome`; ceremony parameters; `thresholdOf` |
| `engine/multisig/quint/broadcast.qnt` | `frequent`, `verify`, well-formedness, the one-step lemma state machine, L1–L6 |
| `engine/multisig/quint/oracle.qnt` | idealised `EchoBroadcast` / `QuorumVote` contract |
| `engine/multisig/quint/keygen.qnt` | ten-stage machine over the oracle, K1–K7 |
| `engine/multisig/quint/seam.qnt` | cross-check that the oracle contract matches concrete `verify_broadcasts` |
| `engine/multisig/quint/harness.qnt` | instantiations (n=4/f=1, n=5/f=1 handover) and runs |
| `engine/multisig/quint/check.sh` | one entry point running every check with its configuration |

Tasks 1–8 deliver a fully verified lemma layer, which is independently valuable: it answers the security review's request to pressure-test the quorum math. Tasks 9+ build the ceremony on top.

---

### Task 1: Toolchain bootstrap and `types.qnt`

**Files:**
- Create: `engine/multisig/quint/types.qnt`
- Create: `engine/multisig/quint/README.md`

**Interfaces:**
- Consumes: nothing.
- Produces: `type Party = int`, `type Value = int`, `type Vote = Sent(Value) | Nothing`, `type Send = { from: Party, to: Party, vote: Vote }`, `type Claim = { by: Party, to: Party, about: Party, vote: Vote }`, `type Opt = Found(Vote) | NoQuorum`, `type Outcome = Agreed(Party -> Vote) | Attributed(Set[Party]) | Unattributed`, `pure def thresholdOf(n: int): int`, `pure val PARTIES/BYZ/HONEST/VALUES/VOTES`.

- [ ] **Step 1: Install the toolchain and confirm versions**

```bash
npm install -g @informalsystems/quint
quint --version          # expect 0.32.0 or later
java -version            # any JDK 17+; Apalache needs it
```

If `quint` is not found after install, it is an nvm PATH issue, not a failed install:

```bash
ls "$(npm root -g)/../bin/quint"     # should exist
export PATH="$(npm root -g)/../bin:$PATH"
```

- [ ] **Step 2: Write `types.qnt`**

```quint
// Shared vocabulary for the multisig ceremony models.
//
// Mirrors engine/multisig/src/client/. Cryptography is abstract: a `Value`
// stands for whatever a stage broadcasts (a commitment, a complaint set),
// distinguishable only by equality.
module types {
  type Party = int
  type Value = int

  // What a party sent in a broadcast round, from a receiver's point of view.
  type Vote = Sent(Value) | Nothing

  // A round-1 send: `from` sent `vote` to `to`.
  type Send = { from: Party, to: Party, vote: Vote }

  // A round-2 claim: `by` tells `to` that it heard `vote` from `about`.
  type Claim = { by: Party, to: Party, about: Party, vote: Vote }

  // Result of find_frequent_element: the value more than `t` reporters agree on.
  type Opt = Found(Vote) | NoQuorum

  // Result of verify_broadcasts at one party.
  type Outcome =
    | Agreed(Party -> Vote)
    | Attributed(Set[Party])
    | Unattributed

  pure val PARTIES: Set[Party] = 1.to(4)
  pure val BYZ: Set[Party] = Set(4)
  pure val HONEST: Set[Party] = PARTIES.exclude(BYZ)
  pure val VALUES: Set[Value] = Set(0, 1)
  pure val VOTES: Set[Vote] = VALUES.map(v => Sent(v)).union(Set(Nothing))

  // threshold_from_share_count (utilities/src/lib.rs:72). Quorum is `count > t`.
  pure def thresholdOf(n: int): int = (2 * n - 1) / 3

  pure val T: int = thresholdOf(PARTIES.size())
}
```

- [ ] **Step 3: Typecheck, and check the threshold against the Rust unit test**

```bash
cd engine/multisig/quint
quint typecheck types.qnt && echo TYPECHECK-OK
printf 'thresholdOf(1)\nthresholdOf(2)\nthresholdOf(3)\nthresholdOf(99)\nthresholdOf(100)\n' \
  | quint repl -r types.qnt::types
```

Expected: `TYPECHECK-OK`, then `0`, `1`, `1`, `65`, `66`. These are exactly the assertions in `test_threshold_for_broadcast_verification` (`engine/multisig/src/client/utils.rs:53`). If they differ, the model is wrong, not the Rust.

- [ ] **Step 4: Write `README.md`**

````markdown
# Quint models of the multisig ceremony

Formal models of the keygen ceremony in `engine/multisig/src/client/`,
checking Byzantine attribution correctness. See
`docs/superpowers/specs/2026-08-19-frost-quint-model-design.md` for the design.

## Setup

```bash
npm install -g @informalsystems/quint
# nvm users: the binary may not be on a non-interactive shell's PATH
export PATH="$(npm root -g)/../bin:$PATH"
```

Apalache (for `quint verify`) and the Rust evaluator (for `quint run`) are
downloaded automatically into `~/.quint` on first use. Apalache needs a JDK.

## Running

```bash
./check.sh            # everything, with the configurations that are known to fit
quint typecheck broadcast.qnt
quint run broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1 --max-samples=20000
quint verify broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1
```

`quint run` samples and is fast (~30k traces/s); it finds bugs but proves
nothing. `quint verify` is exhaustive via Apalache and slow. Use `run` while
developing, `verify` before believing a result.

## Gotchas

- Never write `setOfMaps(D, C).oneOf()`. Apalache rejects it ("Trying to expand
  a set of functions"). `quint run` accepts it, so this only bites at verify
  time. Use `Set[record].powerset().oneOf()` with well-formedness constraints.
- `Option` is not in the standard library.
- There is no `quint eval`; use `quint repl -r file.qnt::module`.
````

- [ ] **Step 5: Commit**

```bash
git add engine/multisig/quint/types.qnt engine/multisig/quint/README.md
git commit -m "test: add Quint vocabulary for the multisig ceremony models"
```

---

### Task 2: `frequent` — the `find_frequent_element` kernel

**Files:**
- Create: `engine/multisig/quint/broadcast.qnt`

**Interfaces:**
- Consumes: everything from `types.qnt`.
- Produces: `pure def frequent(reports: List[Vote], t: int): Opt`.

`find_frequent_element` (`engine/multisig/src/client/utils.rs:26`) returns the first element occurring strictly more than `t` times. Because a quorum exceeds half the reports, at most one element can qualify, so "first" is unambiguous.

- [ ] **Step 1: Write the failing test**

Quint tests are `run` definitions asserting with `assert`. Create `broadcast.qnt`:

```quint
module broadcast {
  import types.* from "./types"

  run frequentTest = all {
    // 3 of 4 agree, t = 2 -> quorum
    assert(frequent([Sent(1), Sent(1), Sent(1), Nothing], 2) == Found(Sent(1))),
    // split 2-2, t = 2 -> no quorum
    assert(frequent([Sent(1), Sent(0), Sent(1), Nothing], 2) == NoQuorum),
    // 3 of 4 agree the party sent nothing -> attributable
    assert(frequent([Nothing, Nothing, Nothing, Sent(1)], 2) == Found(Nothing)),
    // exactly t occurrences is NOT a quorum (strict inequality)
    assert(frequent([Sent(1), Sent(1), Sent(0), Sent(0)], 2) == NoQuorum),
  }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
cd engine/multisig/quint && quint test broadcast.qnt
```

Expected: failure with `Name 'frequent' not found`.

- [ ] **Step 3: Implement `frequent`**

Add above `frequentTest`:

```quint
  // find_frequent_element (client/utils.rs:26): the value strictly more than
  // `t` of the reports agree on, if any. At most one value can qualify.
  pure def frequent(reports: List[Vote], t: int): Opt = {
    val distinct = reports.foldl(Set(), (acc, v) => acc.union(Set(v)))
    val winners = distinct.filter(v => reports.select(w => w == v).length() > t)
    if (winners.size() == 0) NoQuorum else Found(winners.fold(Nothing, (_, v) => v))
  }
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
quint test broadcast.qnt
```

Expected: `frequentTest` passes.

- [ ] **Step 5: Commit**

```bash
git add engine/multisig/quint/broadcast.qnt
git commit -m "test: model find_frequent_element in Quint"
```

---

### Task 3: `verify` — the three-way outcome of `verify_broadcasts`

**Files:**
- Modify: `engine/multisig/quint/broadcast.qnt`

**Interfaces:**
- Consumes: `frequent` from Task 2.
- Produces: `pure def claimed(cs: Set[Claim], by: Party, k: Party, about: Party): Vote`, `pure def sentRound2(cs: Set[Claim], by: Party, k: Party): bool`, `pure def verify(k: Party, cs: Set[Claim]): Outcome`.

This is the security-critical function (`client/common/broadcast_verification.rs`). The three outcomes must stay distinct: a quorum agreeing a party sent *nothing* is attributable and reported; no quorum either way fails the stage and reports **nobody**. Collapsing those two is the bug this whole model exists to rule out.

- [ ] **Step 1: Write the failing test**

Append to `broadcast.qnt`:

```quint
  // Helper for tests: everyone honestly echoes that `about` sent `v`.
  pure def unanimous(about: Party, v: Vote): Set[Claim] =
    tuples(PARTIES, PARTIES).map(t => { by: t._1, to: t._2, about: about, vote: v })

  pure def allClaims(f: (Party) => Vote): Set[Claim] =
    tuples(PARTIES, PARTIES, PARTIES).map(t =>
      { by: t._1, to: t._2, about: t._3, vote: f(t._3) })

  run verifyTest = all {
    // Everyone agrees every party sent its index as a value -> Agreed.
    assert(match verify(1, allClaims(i => Sent(i))) {
      | Agreed(m) => m.get(3) == Sent(3)
      | Attributed(_) => false
      | Unattributed => false
    }),
    // Everyone agrees party 4 sent nothing -> attributable to 4, and only 4.
    assert(match verify(1, allClaims(i => if (i == 4) Nothing else Sent(i))) {
      | Attributed(bad) => bad == Set(4)
      | Agreed(_) => false
      | Unattributed => false
    }),
    // Too few round-2 messages -> Unattributed, nobody blamed.
    assert(verify(1, Set()) == Unattributed),
  }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
quint test broadcast.qnt
```

Expected: failure with `Name 'verify' not found`.

- [ ] **Step 3: Implement `verify` and its helpers**

Add above the tests:

```quint
  // What k heard `by` claim about `about`. Absence of a claim reads as Nothing,
  // matching BroadcastStage::finalize inserting None for unreceived messages.
  pure def claimed(cs: Set[Claim], by: Party, k: Party, about: Party): Vote = {
    val c = cs.filter(x => x.by == by and x.to == k and x.about == about)
    if (c.size() == 1) c.fold(Nothing, (_, x) => x.vote) else Nothing
  }

  pure def sentRound2(cs: Set[Claim], by: Party, k: Party): bool =
    cs.exists(x => x.by == by and x.to == k)

  // verify_broadcasts (broadcast_verification.rs) as executed by party k.
  pure def verify(k: Party, cs: Set[Claim]): Outcome = {
    val present = PARTIES.filter(j => sentRound2(cs, j, k))
    if (present.size() <= T) Unattributed
    else {
      val resolved = PARTIES.mapBy(i =>
        frequent(present.fold(List(), (acc, j) => acc.append(claimed(cs, j, k, i))), T))
      val blamed = PARTIES.filter(i => resolved.get(i) == Found(Nothing))
      val stuck = PARTIES.filter(i => resolved.get(i) == NoQuorum)
      // Order matters: attributable blame wins, and an unresolved value must
      // never become blame. Claims of non-receipt are unfalsifiable, so a
      // colluding minority denying quorum could otherwise frame honest nodes.
      if (blamed.size() > 0) Attributed(blamed)
      else if (stuck.size() > 0) Unattributed
      else Agreed(PARTIES.mapBy(i => match resolved.get(i) {
        | Found(v) => v
        | NoQuorum => Nothing
      }))
    }
  }
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
quint test broadcast.qnt
```

Expected: `frequentTest` and `verifyTest` both pass.

- [ ] **Step 5: Commit**

```bash
git add engine/multisig/quint/broadcast.qnt
git commit -m "test: model verify_broadcasts three-way outcome in Quint"
```

---

### Task 4: The adversary, and verifying L1/L2/L4

**Files:**
- Modify: `engine/multisig/quint/broadcast.qnt`

**Interfaces:**
- Consumes: `verify` from Task 3.
- Produces: state vars `sends: Set[Send]`, `claims: Set[Claim]`, `out: Party -> Outcome`; actions `init`, `step`; invariants `L1_NoFalseBlame`, `L2_ValueAgreement`, `L3_HonestValuePreservation`, `L4_SafeDivergence`, `L5_Liveness`.

The whole ceremony is one step: `init` sets empty state, `step` picks every adversary choice at once and computes each party's outcome. A one-step model is what makes Apalache tractable here.

**This task's encoding is the one that must not use `setOfMaps`.** Honest value assignment, Byzantine round-1 sends, and Byzantine round-2 claims are all subsets of a finite record set, constrained afterwards.

- [ ] **Step 1: Write the failing invariant check**

Append to `broadcast.qnt`:

```quint
  var sends: Set[Send]
  var claims: Set[Claim]
  var out: Party -> Outcome

  // L1: an honest party never blames an honest party. THE headline property.
  val L1_NoFalseBlame = HONEST.forall(k =>
    match out.get(k) {
    | Attributed(bad) => bad.intersect(HONEST) == Set()
    | Agreed(_) => true
    | Unattributed => true
    })

  // L2: two honest parties that both agree, agree on the same map.
  val L2_ValueAgreement = tuples(HONEST, HONEST).forall(p =>
    match out.get(p._1) {
    | Agreed(m1) => match out.get(p._2) {
        | Agreed(m2) => m1 == m2
        | Attributed(_) => true
        | Unattributed => true
      }
    | Attributed(_) => true
    | Unattributed => true
    })

  // L3: an agreed map records what honest senders actually sent.
  val L3_HonestValuePreservation = HONEST.forall(k =>
    match out.get(k) {
    | Agreed(m) => HONEST.forall(i =>
        sends.exists(s => s.from == i and s.to == k and s.vote == m.get(i)))
    | Attributed(_) => true
    | Unattributed => true
    })

  // L4: divergence is permitted between Agreed and failure, but never between
  // two different Agreed maps, and never into blame. Full outcome agreement is
  // FALSE for this protocol - see the spec for the counterexample.
  val L4_SafeDivergence = L1_NoFalseBlame and L2_ValueAgreement
```

- [ ] **Step 2: Run to verify it fails**

```bash
quint run broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1 --max-samples=100
```

Expected: failure — `init`/`step` are not defined yet, so there is no state machine to run.

- [ ] **Step 3: Implement the adversary**

Add above the invariants:

```quint
  pure val BYZ_SENDS: Set[Send] =
    tuples(BYZ, PARTIES, VOTES).map(t => { from: t._1, to: t._2, vote: t._3 })

  pure val BYZ_CLAIMS: Set[Claim] =
    tuples(BYZ, PARTIES, PARTIES, VOTES).map(t =>
      { by: t._1, to: t._2, about: t._3, vote: t._4 })

  // A Byzantine party may send anything, but only one thing per destination.
  pure def wellFormedSends(ss: Set[Send]): bool =
    tuples(BYZ, PARTIES).forall(t =>
      ss.filter(x => x.from == t._1 and x.to == t._2).size() <= 1)

  pure def wellFormedClaims(cs: Set[Claim]): bool =
    tuples(BYZ, PARTIES, PARTIES).forall(t =>
      cs.filter(x => x.by == t._1 and x.to == t._2 and x.about == t._3).size() <= 1)

  action init = all {
    sends' = Set(),
    claims' = Set(),
    out' = PARTIES.mapBy(i => Unattributed),
  }

  action step = {
    // NOTE: powerset, never setOfMaps - Apalache cannot expand sets of functions.
    nondet hvs = tuples(HONEST, VALUES).map(t => { p: t._1, v: t._2 }).powerset().oneOf()
    nondet bs = BYZ_SENDS.powerset().oneOf()
    nondet bc = BYZ_CLAIMS.powerset().oneOf()

    val honestSends = tuples(HONEST, PARTIES).map(t =>
      { from: t._1, to: t._2,
        vote: Sent(hvs.filter(x => x.p == t._1).fold(0, (_, x) => x.v)) })
    val allSends = honestSends.union(bs)

    // An honest party echoes to everyone exactly what it received.
    val honestClaims = tuples(HONEST, PARTIES, PARTIES).map(t => {
      val heard = allSends.filter(s => s.from == t._3 and s.to == t._1)
      { by: t._1, to: t._2, about: t._3,
        vote: if (heard.size() == 1) heard.fold(Nothing, (_, x) => x.vote) else Nothing }
    })
    val allClaims = honestClaims.union(bc)

    all {
      HONEST.forall(h => hvs.filter(x => x.p == h).size() == 1),
      wellFormedSends(allSends),
      wellFormedClaims(allClaims),
      sends' = allSends,
      claims' = allClaims,
      out' = PARTIES.mapBy(k => verify(k, allClaims)),
    }
  }
```

- [ ] **Step 4: Simulate, then verify exhaustively**

```bash
quint run broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1 --max-samples=20000
quint run broadcast.qnt --invariant=L2_ValueAgreement --max-steps=1 --max-samples=20000
quint run broadcast.qnt --invariant=L3_HonestValuePreservation --max-steps=1 --max-samples=20000
```

Expected: `[ok] No violation found` for all three.

Then the exhaustive check. Each takes roughly 2.5 minutes at n=4/f=1; do not interrupt them:

```bash
quint verify broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1
quint verify broadcast.qnt --invariant=L2_ValueAgreement --max-steps=1
```

Expected: `The outcome is: NoError` and `[ok] No violation found` for both. Reference timings from the prototype: L1 144 s, L2 159 s.

If Apalache instead prints *"Trying to expand a set of functions"*, a `setOfMaps` has crept in — find it and convert it to a powerset of records.

- [ ] **Step 5: Add and run the liveness check**

Liveness is conditional — it only holds when nobody deviates — so it is a scenario test rather than an invariant over the free adversary. Append to `broadcast.qnt`:

```quint
  // L5: with no Byzantine deviation (everyone sends one consistent value and
  // echoes truthfully), every party reaches Agreed and recovers what was sent.
  run L5_LivenessTest = {
    val ss = tuples(PARTIES, PARTIES).map(t => { from: t._1, to: t._2, vote: Sent(t._1) })
    val cs = tuples(PARTIES, PARTIES, PARTIES).map(t =>
      { by: t._1, to: t._2, about: t._3, vote: Sent(t._3) })
    assert(PARTIES.forall(k => match verify(k, cs) {
      | Agreed(m) => PARTIES.forall(i => m.get(i) == Sent(i))
      | Attributed(_) => false
      | Unattributed => false
    }))
  }
```

```bash
quint test broadcast.qnt
```

Expected: `L5_LivenessTest` passes alongside the earlier tests. A failure here means the quorum threshold is mis-transcribed — an all-honest run must never fail.

- [ ] **Step 6: Confirm the known counterexample is reproduced**

Add this invariant temporarily to check the model has real adversarial power — a model that cannot find it is too weak to trust:

```quint
  val L4_TooStrong_MustFail = tuples(HONEST, HONEST).forall(p => out.get(p._1) == out.get(p._2))
```

```bash
quint run broadcast.qnt --invariant=L4_TooStrong_MustFail --max-steps=1 --max-samples=20000
```

Expected: `[violation] Found an issue`. The trace should show a Byzantine party equivocating in round 1 and tie-breaking in round 2, leaving some honest parties `Agreed` and others `Unattributed`. Once confirmed, delete `L4_TooStrong_MustFail` — `L4_SafeDivergence` is the property that is kept.

- [ ] **Step 7: Commit**

```bash
git add engine/multisig/quint/broadcast.qnt
git commit -m "test: verify no-false-blame and value agreement for echo broadcast

L1 and L2 verified exhaustively at n=4/f=1 via Apalache. Full outcome
agreement is deliberately not asserted: it is false, and the spec records
the counterexample."
```

---

### Task 5: `QuorumVote` — the single-round primitive, and L6

**Files:**
- Modify: `engine/multisig/quint/broadcast.qnt`

**Interfaces:**
- Consumes: `frequent`, `types.*`.
- Produces: `pure def quorumVote(k: Party, sharers: Set[Party], ss: Set[Send]): Opt`.

`PubkeySharesStage0` (`keygen_stages.rs:129`) is **not** the echo primitive. It is a single round, its threshold is over `sharing_participants.len()` rather than all parties, and every one of its failure paths reports an empty set — a party without the original key cannot tell which pubkey shares are correct. Modelling it as an echo would hand it attribution power it does not have.

- [ ] **Step 1: Write the failing test**

Append to `broadcast.qnt`:

```quint
  run quorumVoteTest = all {
    // 3 sharers all sending the same value: threshold over 3 is 1, so 3 > 1.
    assert(quorumVote(1, Set(1, 2, 3),
      Set({ from: 1, to: 1, vote: Sent(7) },
          { from: 2, to: 1, vote: Sent(7) },
          { from: 3, to: 1, vote: Sent(7) })) == Found(Sent(7))),
    // A 2-1 split over 3 sharers: 2 > 1, so the majority still carries.
    assert(quorumVote(1, Set(1, 2, 3),
      Set({ from: 1, to: 1, vote: Sent(7) },
          { from: 2, to: 1, vote: Sent(7) },
          { from: 3, to: 1, vote: Sent(9) })) == Found(Sent(7))),
    // A 1-1-1 split reaches nothing.
    assert(quorumVote(1, Set(1, 2, 3),
      Set({ from: 1, to: 1, vote: Sent(7) },
          { from: 2, to: 1, vote: Sent(8) },
          { from: 3, to: 1, vote: Sent(9) })) == NoQuorum),
  }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
quint test broadcast.qnt
```

Expected: failure with `Name 'quorumVote' not found`.

- [ ] **Step 3: Implement `quorumVote`**

```quint
  // PubkeySharesStage0 (keygen_stages.rs:129). One round, threshold taken over
  // the SHARING participants only, and it can never attribute: all its failure
  // paths report an empty set.
  pure def quorumVote(k: Party, sharers: Set[Party], ss: Set[Send]): Opt = {
    val t = thresholdOf(sharers.size())
    val received = sharers.fold(List(), (acc, j) => {
      val m = ss.filter(s => s.from == j and s.to == k)
      if (m.size() == 1) acc.append(m.fold(Nothing, (_, x) => x.vote)) else acc
    })
    if (received.length() <= t) NoQuorum else frequent(received, t)
  }
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
quint test broadcast.qnt
```

Expected: all three `run` definitions pass.

- [ ] **Step 5: Add and check L6**

```quint
  // L6: honest parties never disagree on a quorum-vote result, and the
  // primitive never blames anyone (it returns no blame set at all).
  val L6_VoteAgreement = tuples(HONEST, HONEST).forall(p =>
    quorumVote(p._1, PARTIES, sends) == quorumVote(p._2, PARTIES, sends)
    or quorumVote(p._1, PARTIES, sends) == NoQuorum
    or quorumVote(p._2, PARTIES, sends) == NoQuorum)
```

```bash
quint run broadcast.qnt --invariant=L6_VoteAgreement --max-steps=1 --max-samples=20000
quint verify broadcast.qnt --invariant=L6_VoteAgreement --max-steps=1
```

Expected: no violation from either. If `run` finds a violation, capture the trace and stop — that is a real finding about `PubkeySharesStage0` and needs reporting before continuing.

- [ ] **Step 6: Commit**

```bash
git add engine/multisig/quint/broadcast.qnt
git commit -m "test: model PubkeyShares0 single-round quorum vote and verify L6"
```

---

### Task 6: `oracle.qnt` — the idealised contract

**Files:**
- Create: `engine/multisig/quint/oracle.qnt`

**Interfaces:**
- Consumes: `types.*`.
- Produces: `type OracleResult = OkAgreed(Party -> Vote) | OkAttributed(Set[Party]) | OkFailed`, `pure def oracleWellFormed(r: OracleResult, honest: Set[Party]): bool`, `action echoBroadcast`.

This module is the seam between the verified lemma and the ceremony. It states what `broadcast.qnt` proved, so `keygen.qnt` can call it as one atomic step. Writing the contract down explicitly means a future bug at the seam can be traced to a specific violated clause.

- [ ] **Step 1: Write the failing test**

```quint
module oracle {
  import types.* from "./types"

  type OracleResult =
    | OkAgreed(Party -> Vote)
    | OkAttributed(Set[Party])
    | OkFailed

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
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
quint test oracle.qnt
```

Expected: failure with `Name 'oracleWellFormed' not found`.

- [ ] **Step 3: Implement the contract**

Add above the test:

```quint
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
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
quint test oracle.qnt
```

Expected: `oracleContractTest` passes.

- [ ] **Step 5: Commit**

```bash
git add engine/multisig/quint/oracle.qnt
git commit -m "test: state the idealised broadcast oracle contract"
```

---

### Task 7: Seam cross-check — the oracle is no stronger than the lemma

**Files:**
- Create: `engine/multisig/quint/seam.qnt`

**Interfaces:**
- Consumes: `broadcast.*`, `oracle.*`.
- Produces: invariant `SeamSound`.

This is the task that buys back most of what the layered approach gives up against a monolithic model. It checks that every outcome the *concrete* `verify` can actually produce satisfies the *idealised* contract. If the contract were stronger than reality, `keygen.qnt` would be reasoning from a false premise and every K-property would be worthless.

- [ ] **Step 1: Write the failing invariant**

```quint
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
  // satisfy the contract keygen.qnt assumes.
  val SeamSound = HONEST.forall(k => oracleWellFormed(toOracle(out.get(k)), HONEST))
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd engine/multisig/quint && quint typecheck seam.qnt
```

Expected: failure with `Name 'toOracle' not found` or a missing-import error, until the module is written as above.

Cross-module state-variable imports are confirmed to work: `import broadcast.* from "./broadcast"` brings `out` into scope along with `broadcast`'s `init`/`step`, so `quint run seam.qnt` drives the imported state machine directly. No instancing is needed.

- [ ] **Step 3: Make it typecheck and run**

```bash
quint typecheck seam.qnt && echo TYPECHECK-OK
quint run seam.qnt --invariant=SeamSound --max-steps=1 --max-samples=20000
```

Expected: `TYPECHECK-OK`, then `[ok] No violation found`.

- [ ] **Step 4: Verify exhaustively**

```bash
quint verify seam.qnt --invariant=SeamSound --max-steps=1
```

Expected: `The outcome is: NoError`. Budget ~3 minutes.

If this fails, **stop and report**. A violation means the oracle contract is stronger than what `verify` guarantees, and the ceremony model must not be built until the contract is corrected.

- [ ] **Step 5: Commit**

```bash
git add engine/multisig/quint/seam.qnt
git commit -m "test: cross-check the oracle contract against concrete verify_broadcasts"
```

---

### Task 8: `check.sh` and the phase-1 milestone

**Files:**
- Create: `engine/multisig/quint/check.sh`
- Modify: `engine/multisig/quint/README.md`

**Interfaces:**
- Consumes: every module so far.
- Produces: a single command that runs every check at a known-good configuration.

- [ ] **Step 1: Write `check.sh`**

```bash
#!/usr/bin/env bash
# Run every Quint check for the multisig models.
#   ./check.sh          simulation only (fast, ~seconds)
#   ./check.sh --verify  add exhaustive Apalache checks (slow, ~15 minutes)
set -euo pipefail
cd "$(dirname "$0")"

command -v quint >/dev/null || { echo "quint not on PATH; see README.md"; exit 1; }

SIM_INVARIANTS=(
  "broadcast.qnt:L1_NoFalseBlame"
  "broadcast.qnt:L2_ValueAgreement"
  "broadcast.qnt:L3_HonestValuePreservation"
  "broadcast.qnt:L4_SafeDivergence"
  "broadcast.qnt:L6_VoteAgreement"
  "seam.qnt:SeamSound"
)
# Exhaustive checks, each ~2-3 minutes at n=4/f=1.
VERIFY_INVARIANTS=(
  "broadcast.qnt:L1_NoFalseBlame"
  "broadcast.qnt:L2_ValueAgreement"
  "seam.qnt:SeamSound"
)

echo "== typecheck =="
for f in types.qnt broadcast.qnt oracle.qnt seam.qnt; do
  quint typecheck "$f" && echo "  ok $f"
done

echo "== unit tests =="
quint test broadcast.qnt
quint test oracle.qnt

echo "== simulation =="
for entry in "${SIM_INVARIANTS[@]}"; do
  quint run "${entry%%:*}" --invariant="${entry##*:}" --max-steps=1 --max-samples=20000 \
    | grep -E '^\[(ok|violation)\]' | sed "s|^|  ${entry} |"
done

if [[ "${1:-}" == "--verify" ]]; then
  echo "== exhaustive verification (slow) =="
  for entry in "${VERIFY_INVARIANTS[@]}"; do
    echo "  ${entry} ..."
    quint verify "${entry%%:*}" --invariant="${entry##*:}" --max-steps=1 \
      | grep -E '^\[(ok|violation)\]' | sed "s|^|  ${entry} |"
  done
fi
```

- [ ] **Step 2: Make it executable and run it**

```bash
chmod +x engine/multisig/quint/check.sh
./engine/multisig/quint/check.sh
```

Expected: typecheck ok for all four files, both test suites pass, and `[ok]` for all six simulated invariants.

- [ ] **Step 3: Run the full verification**

```bash
./engine/multisig/quint/check.sh --verify
```

Expected: `[ok]` for all three exhaustive checks. Budget ~10 minutes.

- [ ] **Step 4: Record results in the README**

Append to `engine/multisig/quint/README.md`:

```markdown
## Status

Verified exhaustively at n=4 with 1 Byzantine party (`quint verify`):

| Property | Meaning | Time |
| --- | --- | --- |
| L1 NoFalseBlame | an honest node is never blamed | ~144 s |
| L2 ValueAgreement | honest nodes never agree on different values | ~159 s |
| SeamSound | the oracle contract matches concrete `verify_broadcasts` | ~170 s |

Checked by simulation only: L3, L4, L6.

**Not a property of this protocol:** full outcome agreement. A Byzantine party
can equivocate in round 1 and tie-break in round 2, leaving some honest parties
`Agreed` and others failing. This is a liveness degradation, not a safety
violation — see the spec for the counterexample.
```

- [ ] **Step 5: Commit**

```bash
git add engine/multisig/quint/check.sh engine/multisig/quint/README.md
git commit -m "test: add a single entry point for the multisig Quint checks"
```

**Milestone.** Tasks 1–8 deliver the lemma layer with its two central properties exhaustively verified. This is independently valuable and answers the security review's request to pressure-test the quorum math. Stop here for review before building the ceremony layer.

---

### Task 9: `keygen.qnt` — stages 0–5 over the oracle

**Files:**
- Create: `engine/multisig/quint/keygen.qnt`

**Interfaces:**
- Consumes: `types.*`, `oracle.*`.
- Produces: `type Stage`, `type CeremonyOutcome = Running | Done(Party -> Vote) | Failed(Set[Party])`, state vars `stage: Party -> Stage`, `result: Party -> CeremonyOutcome`, `shareValid: Set[{ from: Party, to: Party }]`; invariants `K1_NoHonestBlamed`, `K2_NoConflictingOutcome`, `K5_Termination`.

Model the stage sequence as a per-party program counter. Each `Verify*` stage consumes one oracle result; each non-verify stage advances. Secret shares are private and unverifiable, so `shareValid` is an adversary-chosen relation for Byzantine senders and always true for honest ones.

- [ ] **Step 1: Write the failing test**

```quint
module keygen {
  import types.* from "./types"
  import oracle.* from "./oracle"

  type Stage =
    | PubkeyShares0 | HashCommitments1 | VerifyHashCommitments2
    | CoefficientCommitments3 | VerifyCommitments4 | SecretShares5
    | Complaints6 | VerifyComplaints7 | BlameResponses8 | VerifyBlameResponses9
    | Finished

  type CeremonyOutcome = Running | Done(Party -> Vote) | Failed(Set[Party])

  run stageOrderTest = all {
    assert(nextStage(PubkeyShares0) == HashCommitments1),
    assert(nextStage(VerifyCommitments4) == SecretShares5),
    assert(nextStage(VerifyBlameResponses9) == Finished),
    assert(nextStage(Finished) == Finished),
  }
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
quint test keygen.qnt
```

Expected: failure with `Name 'nextStage' not found`.

- [ ] **Step 3: Implement the stage sequence**

Add above the test. The order mirrors `KeygenStageName` (`client/common.rs:249`):

```quint
  pure def nextStage(s: Stage): Stage =
    match s {
    | PubkeyShares0 => HashCommitments1
    | HashCommitments1 => VerifyHashCommitments2
    | VerifyHashCommitments2 => CoefficientCommitments3
    | CoefficientCommitments3 => VerifyCommitments4
    | VerifyCommitments4 => SecretShares5
    | SecretShares5 => Complaints6
    | Complaints6 => VerifyComplaints7
    | VerifyComplaints7 => BlameResponses8
    | BlameResponses8 => VerifyBlameResponses9
    | VerifyBlameResponses9 => Finished
    | Finished => Finished
    }

  // Stages that consume an echo-broadcast oracle result.
  pure def isVerifyStage(s: Stage): bool =
    s == VerifyHashCommitments2 or s == VerifyCommitments4
      or s == VerifyComplaints7 or s == VerifyBlameResponses9
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
quint test keygen.qnt
```

Expected: `stageOrderTest` passes.

- [ ] **Step 5: Add the state machine and K1/K2/K5**

Append:

```quint
  var stage: Party -> Stage
  var result: Party -> CeremonyOutcome

  action init = all {
    stage' = PARTIES.mapBy(_ => PubkeyShares0),
    result' = PARTIES.mapBy(_ => Running),
  }

  // One stage transition. Verify stages consume an oracle result honouring the
  // contract; other stages advance unconditionally.
  action step = {
    nondet r = Set(OkAgreed(PARTIES.mapBy(i => Sent(i))),
                   OkAttributed(BYZ),
                   OkFailed).oneOf()
    all {
      oracleWellFormed(r, HONEST),
      stage' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) stage.get(k)
        else if (isVerifyStage(stage.get(k)) and r != OkAgreed(PARTIES.mapBy(i => Sent(i))))
          stage.get(k)
        else nextStage(stage.get(k))),
      result' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) result.get(k)
        else if (isVerifyStage(stage.get(k)))
          match r {
          | OkAgreed(_) => if (nextStage(stage.get(k)) == Finished)
                             Done(PARTIES.mapBy(i => Sent(i))) else Running
          | OkAttributed(bad) => Failed(bad)
          | OkFailed => Failed(Set())
          }
        else if (nextStage(stage.get(k)) == Finished)
          Done(PARTIES.mapBy(i => Sent(i)))
        else Running),
    }
  }

  // K1: no honest party is blamed by any honest party. THE headline property.
  val K1_NoHonestBlamed = HONEST.forall(k =>
    match result.get(k) {
    | Failed(bad) => bad.intersect(HONEST) == Set()
    | Done(_) => true
    | Running => true
    })

  // K2: no two honest parties finish with different keys.
  val K2_NoConflictingOutcome = tuples(HONEST, HONEST).forall(p =>
    match result.get(p._1) {
    | Done(k1) => match result.get(p._2) { Done(k2) => k1 == k2 | Failed(_) => true | Running => true }
    | Failed(_) => true
    | Running => true
    })

  // K5: no honest party is stuck Running once it reaches Finished.
  val K5_Termination = HONEST.forall(k =>
    stage.get(k) != Finished or result.get(k) != Running)
```

- [ ] **Step 6: Check**

```bash
quint typecheck keygen.qnt && echo TYPECHECK-OK
for inv in K1_NoHonestBlamed K2_NoConflictingOutcome K5_Termination; do
  quint run keygen.qnt --invariant=$inv --max-steps=12 --max-samples=20000 \
    | grep -E '^\[(ok|violation)\]' | sed "s|^|$inv |"
done
```

Expected: `TYPECHECK-OK` and `[ok]` for all three. `--max-steps=12` covers the ten stages plus slack.

- [ ] **Step 7: Commit**

```bash
git add engine/multisig/quint/keygen.qnt
git commit -m "test: model keygen stage machine over the broadcast oracle"
```

---

### Task 10: Secret shares, complaints, and blame (stages 5–9)

**Files:**
- Modify: `engine/multisig/quint/keygen.qnt`

**Interfaces:**
- Consumes: Task 9's stage machine.
- Produces: state var `shareValid: Set[{ from: Party, to: Party }]`, `complaints: Set[{ by: Party, about: Party }]`, `revealed: Set[{ by: Party, about: Party, ok: bool }]`; `pure def blameResponseComplete(...)`; invariant `K3_AttributionProgress`.

This is the second, independent attribution mechanism — the one not covered by the echo lemma, and the richest source of false-blame bugs. Stage 5 shares are private, so a Byzantine sender can send a bad share undetectably; stage 6 lets the receiver complain; stage 8 forces the blamed party to reveal; stage 9 re-verifies and attributes.

- [ ] **Step 1: Write the failing test**

Append to `keygen.qnt`:

```quint
  run blameResponseTest = all {
    // A response must contain a share for exactly the parties that complained.
    assert(blameResponseComplete(4,
      Set({ by: 1, about: 4 }, { by: 2, about: 4 }),
      Set({ by: 4, about: 1, ok: true }, { by: 4, about: 2, ok: true }))),
    // Missing one complainant's share -> incomplete.
    assert(not(blameResponseComplete(4,
      Set({ by: 1, about: 4 }, { by: 2, about: 4 }),
      Set({ by: 4, about: 1, ok: true })))),
    // Revealing a share nobody asked for -> incomplete.
    assert(not(blameResponseComplete(4,
      Set({ by: 1, about: 4 }),
      Set({ by: 4, about: 1, ok: true }, { by: 4, about: 3, ok: true })))),
  }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
quint test keygen.qnt
```

Expected: failure with `Name 'blameResponseComplete' not found`.

- [ ] **Step 3: Implement**

```quint
  // is_blame_response_complete (keygen_stages.rs): the response must contain a
  // share for exactly the set of parties that complained about `sender`.
  pure def blameResponseComplete(
    sender: Party,
    cs: Set[{ by: Party, about: Party }],
    rs: Set[{ by: Party, about: Party, ok: bool }]
  ): bool = {
    val expected = cs.filter(c => c.about == sender).map(c => c.by)
    val actual = rs.filter(r => r.by == sender).map(r => r.about)
    expected == actual
  }
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
quint test keygen.qnt
```

Expected: `blameResponseTest` passes.

- [ ] **Step 5: Wire shares and complaints into the state machine**

Add state variables and extend `init`/`step`. An honest party complains exactly when a share it received was invalid; a Byzantine party may complain arbitrarily:

```quint
  var shareValid: Set[{ from: Party, to: Party }]
  var complaints: Set[{ by: Party, about: Party }]

  pure val ALL_PAIRS: Set[{ from: Party, to: Party }] =
    tuples(PARTIES, PARTIES).map(t => { from: t._1, to: t._2 })

  // Honest senders always send valid shares; Byzantine senders choose freely.
  action chooseShares = {
    nondet bad = tuples(BYZ, PARTIES).map(t => { from: t._1, to: t._2 }).powerset().oneOf()
    shareValid' = ALL_PAIRS.exclude(bad)
  }

  // Honest k complains about exactly the senders whose share to k was invalid.
  // Byzantine parties may complain about anyone.
  action chooseComplaints = {
    nondet byzComplaints =
      tuples(BYZ, PARTIES).map(t => { by: t._1, about: t._2 }).powerset().oneOf()
    complaints' =
      tuples(HONEST, PARTIES)
        .filter(t => not(shareValid.contains({ from: t._2, to: t._1 })))
        .map(t => { by: t._1, about: t._2 })
        .union(byzComplaints)
  }

  // K3: blame must be productive as well as safe - a non-empty reported set
  // contains at least one Byzantine party, or an adversary can force endless
  // unattributed retries.
  val K3_AttributionProgress = HONEST.forall(k =>
    match result.get(k) {
    | Failed(bad) => bad == Set() or bad.intersect(BYZ) != Set()
    | Done(_) => true
    | Running => true
    })
```

Then extend `init` and `step`. `init` gains two conjuncts:

```quint
    shareValid' = ALL_PAIRS,
    complaints' = Set(),
```

and `step` gains two, so that every transition assigns every state variable (Quint requires this) while only the relevant stage actually changes them:

```quint
      shareValid' = if (PARTIES.exists(k => stage.get(k) == SecretShares5))
                      ALL_PAIRS.exclude(byzBadShares) else shareValid,
      complaints' = if (PARTIES.exists(k => stage.get(k) == Complaints6))
                      honestComplaints.union(byzComplaints) else complaints,
```

with the two `nondet` picks hoisted to the top of `step` alongside the existing oracle pick:

```quint
    nondet byzBadShares =
      tuples(BYZ, PARTIES).map(t => { from: t._1, to: t._2 }).powerset().oneOf()
    nondet byzComplaints =
      tuples(BYZ, PARTIES).map(t => { by: t._1, about: t._2 }).powerset().oneOf()
    val honestComplaints =
      tuples(HONEST, PARTIES)
        .filter(t => not(shareValid.contains({ from: t._2, to: t._1 })))
        .map(t => { by: t._1, about: t._2 })
```

Both picks are powersets of record sets — never `setOfMaps`. Delete the standalone `chooseShares` / `chooseComplaints` actions shown above; they are inlined here.

- [ ] **Step 6: Check**

```bash
quint typecheck keygen.qnt && echo TYPECHECK-OK
for inv in K1_NoHonestBlamed K2_NoConflictingOutcome K3_AttributionProgress K5_Termination; do
  quint run keygen.qnt --invariant=$inv --max-steps=12 --max-samples=20000 \
    | grep -E '^\[(ok|violation)\]' | sed "s|^|$inv |"
done
```

Expected: `[ok]` for all four. **If K1 or K3 reports a violation, stop and report it** — a false-blame path through the complaint/blame sub-protocol is exactly what this model exists to find, and it needs a human decision before any further modelling.

- [ ] **Step 7: Commit**

```bash
git add engine/multisig/quint/keygen.qnt
git commit -m "test: model keygen complaint and blame sub-protocol"
```

---

### Task 11: Coefficient commitment length and K6

**Files:**
- Modify: `engine/multisig/quint/keygen.qnt`

**Interfaces:**
- Consumes: Task 10's model.
- Produces: `pure def commitmentLengthValid(len: int, keyThreshold: int): bool`, state var `coeffLen: Party -> int`, invariant `K6_KeyConsistency`.

This models review finding #1 directly. Every honest party commits to exactly `threshold + 1` coefficients for the key being generated (`validate_commitments`, `keygen_detail.rs`); under handover that count derives from the **new** key's threshold. A malicious party committing to a different length, with an otherwise valid ZKP and matching hash commitment, would otherwise either raise the polynomial degree (finalising an internally inconsistent key, so the first signing fails for everyone with no attribution) or send a too-short vector (out-of-bounds index panic on honest nodes).

The check is an integer comparison, so it stays faithful in the model rather than assumed.

- [ ] **Step 1: Write the failing test**

Append to `keygen.qnt`:

```quint
  run coeffLengthTest = all {
    // threshold 2 -> exactly 3 coefficients
    assert(commitmentLengthValid(3, 2)),
    assert(not(commitmentLengthValid(4, 2))),   // over-long: raises the degree
    assert(not(commitmentLengthValid(2, 2))),   // too short: OOB on honest nodes
    assert(not(commitmentLengthValid(0, 2))),   // empty
  }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
quint test keygen.qnt
```

Expected: failure with `Name 'commitmentLengthValid' not found`.

- [ ] **Step 3: Implement**

```quint
  // The key being generated. Under handover this is the NEW key's threshold,
  // not the current ceremony's party count.
  pure val KEY_THRESHOLD: int = thresholdOf(PARTIES.size())

  // validate_commitments (keygen_detail.rs): an honest party commits to exactly
  // `threshold + 1` coefficients. Set to false to reproduce the pre-fix
  // behaviour (the global byte cap only) and watch K6 fail.
  pure val ENFORCE_COEFF_LENGTH: bool = true

  pure def commitmentLengthValid(len: int, keyThreshold: int): bool =
    len == keyThreshold + 1

  pure def commitmentAccepted(len: int): bool =
    not(ENFORCE_COEFF_LENGTH) or commitmentLengthValid(len, KEY_THRESHOLD)
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
quint test keygen.qnt
```

Expected: `coeffLengthTest` passes.

- [ ] **Step 5: Wire lengths into the state machine and add K6**

Add the state variable, an adversary choice of lengths, and the stage-4 rejection. Honest parties always commit to the correct count; Byzantine parties pick any length in a small range around it:

```quint
  var coeffLen: Party -> int

  pure val LENGTH_CHOICES: Set[int] = Set(0, KEY_THRESHOLD, KEY_THRESHOLD + 1, KEY_THRESHOLD + 2)
```

In `init`: `coeffLen' = PARTIES.mapBy(_ => KEY_THRESHOLD + 1),`

In `step`, hoisted alongside the other picks:

```quint
    nondet byzLens = tuples(BYZ, LENGTH_CHOICES).map(t => { p: t._1, n: t._2 }).powerset().oneOf()
    val newLens = PARTIES.mapBy(i =>
      if (BYZ.contains(i))
        byzLens.filter(x => x.p == i).fold(KEY_THRESHOLD + 1, (_, x) => x.n)
      else KEY_THRESHOLD + 1)
```

with `BYZ.forall(b => byzLens.filter(x => x.p == b).size() == 1)` as a conjunct, `coeffLen' = if (PARTIES.exists(k => stage.get(k) == CoefficientCommitments3)) newLens else coeffLen,` as the assignment, and the `VerifyCommitments4` branch failing with the offending parties attributed when any `commitmentAccepted` is false.

```quint
  // K6: a ceremony can never finalise with a wrong-length commitment in play.
  // This is review finding #1 stated as an invariant: "keygen finalises, first
  // signing fails for everyone" must be unreachable.
  val K6_KeyConsistency = HONEST.forall(k =>
    match result.get(k) {
    | Done(_) => PARTIES.forall(i => commitmentLengthValid(coeffLen.get(i), KEY_THRESHOLD))
    | Failed(_) => true
    | Running => true
    })
```

- [ ] **Step 6: Check, then confirm the model can see the bug**

```bash
quint typecheck keygen.qnt && echo TYPECHECK-OK
quint run keygen.qnt --invariant=K6_KeyConsistency --max-steps=12 --max-samples=20000 \
  | grep -E '^\[(ok|violation)\]'
```

Expected: `[ok]`.

Now set `ENFORCE_COEFF_LENGTH = false` and re-run:

```bash
quint run keygen.qnt --invariant=K6_KeyConsistency --max-steps=12 --max-samples=50000 \
  | grep -E '^\[(ok|violation)\]'
```

Expected: `[violation] Found an issue`, with a trace where a Byzantine party commits to `KEY_THRESHOLD + 2` coefficients and the ceremony still reaches `Done`. That is finding #1 reproduced in the model.

**If this still reports `[ok]`, the model is too weak to have found the bug the review identified** — fix it before continuing. Restore `true` afterwards.

- [ ] **Step 7: Commit**

```bash
git add engine/multisig/quint/keygen.qnt
git commit -m "test: model the per-ceremony coefficient length check and K6

Includes a negative control: disabling the length check reproduces review
finding #1 - a ceremony finalising with an over-long commitment vector."
```

---

### Task 12: Handover, K4 and K7

**Files:**
- Modify: `engine/multisig/quint/keygen.qnt`
- Create: `engine/multisig/quint/harness.qnt`

**Interfaces:**
- Consumes: Task 10's model.
- Produces: `SHARING`, `RECEIVING`, `futureIndex`; invariants `K4_HandoverNoFalseBlame`, `K6_KeyConsistency`, `K7_StageDivergenceSafety`.

Handover splits parties into sharers and receivers with an index remapping. `VerifyComplaintsBroadcastStage7` already discards complaints from non-receiving participants, because acting on one forces the blamed party to reveal a share at a bogus index, fail stage-9 verification, and be wrongly attributed. **The model must not bake that fix in as an assumption** — it should be able to express the unfixed behaviour, so K4 demonstrably fails without the filter and passes with it.

- [ ] **Step 1: Write the failing invariant**

Create `harness.qnt`:

```quint
// Instantiations. Plain keygen is the case where every party shares and receives.
module harness {
  import types.* from "./types"
  import keygen.* from "./keygen"

  // n=5, f=1, sharers {1,2,3} and receivers {3,4,5}. Deliberately tight: a
  // quorum over three sharers is two, so there is no slack if the Byzantine
  // party is a sharer.
  pure val SHARING: Set[Party] = Set(1, 2, 3)
  pure val RECEIVING: Set[Party] = Set(3, 4, 5)

  // K4: no honest party is blamed under handover.
  val K4_HandoverNoFalseBlame = K1_NoHonestBlamed

  // K7: a subset of honest parties proceeding past a stage while others abort
  // must not be walkable to a finalised key.
  val K7_StageDivergenceSafety =
    HONEST.forall(a => HONEST.forall(b =>
      match result.get(a) {
      | Done(_) => match result.get(b) { Failed(_) => false | Done(_) => true | Running => true }
      | Failed(_) => true
      | Running => true
      }))
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd engine/multisig/quint && quint typecheck harness.qnt
```

Expected: failure — `keygen.qnt` has no notion of sharers or receivers yet.

- [ ] **Step 3: Add handover to `keygen.qnt`**

Parameterise the complaint rule so the non-receiver filter is a *modelled behaviour*, not a hard-coded assumption:

```quint
  // Complaints from non-receiving participants must be discarded. A non-receiver
  // processes no shares, so any complaint from one is necessarily dishonest;
  // acting on it forces the blamed party to reveal a share at an index that
  // then fails stage-9 verification, wrongly attributing a possibly honest node.
  // Set to false to reproduce the unfixed behaviour and watch K4 fail.
  pure val FILTER_NON_RECEIVER_COMPLAINTS: bool = true

  pure def admissibleComplaints(
    cs: Set[{ by: Party, about: Party }],
    receiving: Set[Party]
  ): Set[{ by: Party, about: Party }] =
    if (FILTER_NON_RECEIVER_COMPLAINTS) cs.filter(c => receiving.contains(c.by)) else cs
```

Then replace every use of `complaints` in the stage-7 transition with `admissibleComplaints(complaints, RECEIVING)`. Concretely, the `blameResponseComplete` call in the `VerifyBlameResponses9` branch becomes:

```quint
      blameResponseComplete(sender, admissibleComplaints(complaints, RECEIVING), revealed)
```

and `RECEIVING` must be passed into `keygen.qnt` as a `pure val` (defaulting to `PARTIES`, which is exactly plain keygen) rather than imported from `harness.qnt`, to avoid a circular import:

```quint
  // Overridden per-instantiation. Plain keygen: every party receives.
  pure val RECEIVING: Set[Party] = PARTIES
```

- [ ] **Step 4: Check with the fix on**

```bash
quint typecheck harness.qnt && echo TYPECHECK-OK
for inv in K4_HandoverNoFalseBlame K7_StageDivergenceSafety; do
  quint run harness.qnt --invariant=$inv --max-steps=12 --max-samples=20000 \
    | grep -E '^\[(ok|violation)\]' | sed "s|^|$inv |"
done
```

Expected: `[ok]` for both.

- [ ] **Step 5: Confirm the model can see the bug**

Set `FILTER_NON_RECEIVER_COMPLAINTS = false` and re-run:

```bash
quint run harness.qnt --invariant=K4_HandoverNoFalseBlame --max-steps=12 --max-samples=50000 \
  | grep -E '^\[(ok|violation)\]'
```

Expected: `[violation] Found an issue`, with a trace in which a non-receiving Byzantine party complains about an honest sharer and the honest sharer ends up in a reported set.

**If this still reports `[ok]`, the model is too weak** — it cannot express the bug the Rust already fixes, so it cannot be trusted to find similar ones. Fix the model before continuing. Restore `true` afterwards.

- [ ] **Step 6: Commit**

```bash
git add engine/multisig/quint/keygen.qnt engine/multisig/quint/harness.qnt
git commit -m "test: model key handover and verify no false blame under resharing

Includes a negative control: disabling the non-receiver complaint filter
reproduces the false-blame bug that VerifyComplaintsBroadcastStage7 fixes."
```

---

### Task 13: Wire everything into `check.sh` and record results

**Files:**
- Modify: `engine/multisig/quint/check.sh`
- Modify: `engine/multisig/quint/README.md`
- Modify: `docs/superpowers/specs/2026-08-19-frost-quint-model-design.md`

- [ ] **Step 1: Extend `check.sh`**

Add to `SIM_INVARIANTS`:

```bash
  "keygen.qnt:K1_NoHonestBlamed"
  "keygen.qnt:K2_NoConflictingOutcome"
  "keygen.qnt:K3_AttributionProgress"
  "keygen.qnt:K5_Termination"
  "keygen.qnt:K6_KeyConsistency"
  "harness.qnt:K4_HandoverNoFalseBlame"
  "harness.qnt:K7_StageDivergenceSafety"
```

The ceremony invariants need more steps than the lemma's single one, so give them their own loop rather than reusing `--max-steps=1`:

```bash
CEREMONY_STEPS=12
```

and use `--max-steps="$CEREMONY_STEPS"` for the `keygen.qnt` and `harness.qnt` entries.

Add `keygen.qnt` and `harness.qnt` to the typecheck list, and `quint test keygen.qnt` to the unit-test section.

- [ ] **Step 2: Run the whole suite**

```bash
./engine/multisig/quint/check.sh
./engine/multisig/quint/check.sh --verify
```

Expected: `[ok]` throughout. Budget ~15 minutes for the verify pass.

- [ ] **Step 3: Record actual results honestly**

Update the README status table with the properties that were **exhaustively verified** versus those **only simulated**. Do not describe a simulated property as verified — `quint run` samples and proves nothing. If a configuration had to be reduced to fit, say which and by how much.

- [ ] **Step 4: Update the spec's status**

Change the spec header from `design approved, not yet implemented` to `implemented` and add a line pointing at `engine/multisig/quint/README.md` for current results.

- [ ] **Step 5: Commit**

```bash
git add engine/multisig/quint/check.sh engine/multisig/quint/README.md \
        docs/superpowers/specs/2026-08-19-frost-quint-model-design.md
git commit -m "test: run the full multisig Quint suite from one entry point"
```

---

## Deferred

Not in this plan, listed so they are not mistakenly assumed done:

- **CI integration.** Apalache runs take minutes each; wiring them into PR CI needs a decision about which configurations run per-PR versus nightly. Raise it once the real total runtime is known from Task 13.
- **Counterexample-to-Rust workflow.** `client/helpers.rs` drives ceremonies at message granularity (`gather_outgoing_messages`, `distribute_messages_with_non_sender`, `run_stage_with_non_sender`), so traces map onto it directly — but only worth building once the model has produced a finding worth replaying.
- **Signing ceremony.** Reuses the same echo primitive, so the lemma layer applies unchanged; the four-stage machine would be a much smaller job than keygen.
- **n=7/f=2.** The prototype measured n=4/f=1 at ~2.5 minutes per property. n=7 grows the adversary's claim space from 4·4·4·3 to 7·7·7·3 records; exhaustive verification may be infeasible. Try it, and if it does not complete, record the timeout rather than quietly dropping the configuration.
