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
- **No rejection sampling, anywhere, ever.** This is the single most important rule in this plan and it was violated twice, each time producing results that looked fine and meant nothing. A constraint inside `all { ... }` does not restrict a choice — it *disables the transition*, and the simulator then has to redraw. In a one-step model that yields a vacuous `[ok]` (`step` never fires); in a multi-step model the chance of completing a trace falls off geometrically with depth — measured at 84 s for 8 steps, 217 s for 9, and over 9 minutes for 10, versus **133 ms for 2000 traces at 12 steps** once the rejection was removed. Always decode a draw into a *valid* configuration instead of drawing freely and filtering. State-dependent requirements are decoded too: gate on the state (`if (canAgree and ...)`) rather than asserting it.
- **Encode every adversary choice as characteristic-function subsets, so that every draw is well-formed by construction.** Never draw an arbitrary subset and then reject the ill-formed ones with a guard inside `all { ... }`. A guard makes the simulator reject essentially every draw (a valid round-2 claim set has probability ~2⁻¹⁶), `step` becomes unreachable, and every `quint run` reports a vacuous `[ok]`. To choose one of `k` options per slot, use `⌈log₂ k⌉` independent subsets of the slot set and decode them; to choose "absent or one of two values", use one `present` subset and one `which-value` subset.
- **Every `quint run` must carry `--witnesses`, and every witness must be reached in > 0 traces.** An invariant that is never violated proves nothing if the interesting states were never reached; a witness reported as `0 trace(s) out of N explored (0.00%)` means the state is unreachable — an over-constrained action or a broken encoding — and must be fixed before the accompanying `[ok]` means anything. Also read the `Trace length statistics: max=` line: `max=1` in a one-step model means `step` never fired at all.
- **`quint run`/`verify`/`test` need concrete `const`s.** A module declaring `const` cannot be run directly — it fails `QNT500: Uninitialized const`. Bind them in a small instance module and pass `--main <instance>`. `assume` does NOT bind a const.
- **Simulation is not proof.** Report a `quint run` `[ok]` as "no counterexample in N sampled traces", never as "the property holds". Only `quint verify` justifies the stronger claim.
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
- Produces: `pure def claimed(cs: Set[Claim], by: Party, k: Party, about: Party): Vote`, `pure def sentRound2(ps: Set[(Party, Party)], by: Party, k: Party): bool`, `pure def verify(k: Party, ps: Set[(Party, Party)], cs: Set[Claim]): Outcome`.

Note the explicit `ps` (present) argument. A round-2 message carries an entry for *every* party — the Rust drops any verification message whose key set is not exactly the participant set (`check_verification_message_indexes`) — so "this party sent a message that reports Nothing about everyone" and "this party sent no message" are different situations that must not be conflated. `ps` records which messages exist; `cs` records what they say.

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

  pure val ALL_PRESENT: Set[(Party, Party)] = tuples(PARTIES, PARTIES)

  run verifyTest = all {
    // Everyone agrees every party sent its index as a value -> Agreed.
    assert(match verify(1, ALL_PRESENT, allClaims(i => Sent(i))) {
      | Agreed(m) => m.get(3) == Sent(3)
      | Attributed(_) => false
      | Unattributed => false
    }),
    // Everyone agrees party 4 sent nothing -> attributable to 4, and only 4.
    assert(match verify(1, ALL_PRESENT, allClaims(i => if (i == 4) Nothing else Sent(i))) {
      | Attributed(bad) => bad == Set(4)
      | Agreed(_) => false
      | Unattributed => false
    }),
    // Too few round-2 messages -> Unattributed, nobody blamed.
    assert(verify(1, Set(), Set()) == Unattributed),
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

  // A round-2 message carries an entry for every party (the Rust rejects any
  // whose key set is not exactly the participant set), so presence is tracked
  // separately from content: a message reporting Nothing about everyone is
  // present, and is not the same as no message at all.
  pure def sentRound2(ps: Set[(Party, Party)], by: Party, k: Party): bool =
    ps.contains((by, k))

  // verify_broadcasts (broadcast_verification.rs) as executed by party k.
  pure def verify(k: Party, ps: Set[(Party, Party)], cs: Set[Claim]): Outcome = {
    val present = PARTIES.filter(j => sentRound2(ps, j, k))
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
- Produces: state vars `sends: Set[Send]`, `claims: Set[Claim]`, `present: Set[(Party, Party)]`, `out: Party -> Outcome`; actions `init`, `step`; invariants `L1_NoFalseBlame`, `L2_ValueAgreement`, `L3_HonestValuePreservation`, `L4_SafeDivergence`; and the scenario test `run L5_LivenessTest` (liveness is conditional on no Byzantine deviation, so it is a test, not an invariant over the free adversary).

The whole ceremony is one step: `init` sets empty state, `step` picks every adversary choice at once and computes each party's outcome. A one-step model is what makes Apalache tractable here.

**This task's encoding is the one that decides whether any later result means anything.** Honest value assignment, Byzantine round-1 sends, and Byzantine round-2 claims are each chosen by *characteristic-function subsets* — one subset per bit of choice, decoded into the message sets. Never `setOfMaps` (Apalache rejects it), and never "draw an arbitrary subset, then reject the ill-formed ones with a guard": that was measured to make `step` unreachable, leaving every `quint run` vacuously `[ok]` while the negative control that must fail also reported `[ok]`.

- [ ] **Step 1: Write the failing invariant check**

Append to `broadcast.qnt`:

```quint
  var sends: Set[Send]
  var claims: Set[Claim]
  var present: Set[(Party, Party)]
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

  // Witnesses. These are not properties to hold - they are coverage checks.
  // Each must be reached in > 0 traces, or the run's `[ok]` on the invariants
  // above is vacuous because the interesting states were never explored.
  val wAgreed = HONEST.exists(k =>
    match out.get(k) { Agreed(_) => true | Attributed(_) => false | Unattributed => false })
  val wAttributed = HONEST.exists(k =>
    match out.get(k) { Attributed(_) => true | Agreed(_) => false | Unattributed => false })
  val wUnattributed = HONEST.exists(k =>
    match out.get(k) { Unattributed => true | Agreed(_) => false | Attributed(_) => false })
  // The verified L4 divergence: two honest parties reaching different outcomes.
  val wDiverged = tuples(HONEST, HONEST).exists(p => out.get(p._1) != out.get(p._2))
```

- [ ] **Step 2: Run to verify it fails**

```bash
quint run broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1 --max-samples=100
```

Expected: failure — `init`/`step` are not defined yet, so there is no state machine to run.

- [ ] **Step 3: Implement the adversary**

Add above the invariants:

```quint
  // Slots an adversary may act on. Choices are encoded as characteristic-
  // function subsets of these, so EVERY draw is well-formed by construction.
  // Drawing arbitrary subsets and rejecting the ill-formed ones with a guard
  // makes the simulator reject ~every draw (a valid claim set has probability
  // about 2^-16), leaving `step` unreachable and every run vacuously ok.
  pure val SEND_SLOTS: Set[(Party, Party)] = tuples(BYZ, PARTIES)
  pure val CLAIM_SLOTS: Set[(Party, Party, Party)] = tuples(BYZ, PARTIES, PARTIES)

  action init = all {
    sends' = Set(),
    claims' = Set(),
    present' = Set(),
    out' = PARTIES.mapBy(i => Unattributed),
  }

  action step = {
    // One bit per honest party: which of the two values it broadcasts.
    nondet hvHigh = HONEST.powerset().oneOf()
    // Byzantine round 1: sent-at-all, and which value.
    nondet bsMade = SEND_SLOTS.powerset().oneOf()
    nondet bsHigh = SEND_SLOTS.powerset().oneOf()
    // Byzantine round 2: which messages exist, then per-slot content.
    nondet bcPresent = SEND_SLOTS.powerset().oneOf()
    nondet bcMade = CLAIM_SLOTS.powerset().oneOf()
    nondet bcHigh = CLAIM_SLOTS.powerset().oneOf()

    val honestSends = tuples(HONEST, PARTIES).map(t =>
      { from: t._1, to: t._2, vote: Sent(if (hvHigh.contains(t._1)) 1 else 0) })
    val byzSends = SEND_SLOTS.filter(sl => bsMade.contains(sl)).map(sl =>
      { from: sl._1, to: sl._2, vote: Sent(if (bsHigh.contains(sl)) 1 else 0) })
    val allSends = honestSends.union(byzSends)

    // An honest party echoes to everyone exactly what it received.
    val honestClaims = tuples(HONEST, PARTIES, PARTIES).map(t => {
      val heard = allSends.filter(s => s.from == t._3 and s.to == t._1)
      { by: t._1, to: t._2, about: t._3,
        vote: if (heard.size() == 1) heard.fold(Nothing, (_, x) => x.vote) else Nothing }
    })
    // A slot the Byzantine party omits reads as Nothing (it claims it heard
    // nothing from that party) - the message still exists if bcPresent says so.
    val byzClaims = CLAIM_SLOTS.filter(sl => bcMade.contains(sl)).map(sl =>
      { by: sl._1, to: sl._2, about: sl._3,
        vote: if (bcHigh.contains(sl)) Sent(1) else Sent(0) })
    val allClaims = honestClaims.union(byzClaims)

    // Honest parties always send their round-2 message to everyone.
    val allPresent = tuples(HONEST, PARTIES).union(bcPresent)

    all {
      sends' = allSends,
      claims' = allClaims,
      present' = allPresent,
      out' = PARTIES.mapBy(k => verify(k, allPresent, allClaims)),
    }
  }
```

- [ ] **Step 4: Simulate, then verify exhaustively**

```bash
for inv in L1_NoFalseBlame L2_ValueAgreement L3_HonestValuePreservation; do
  quint run broadcast.qnt --invariant=$inv \
    --witnesses wAgreed wAttributed wUnattributed wDiverged \
    --max-steps=1 --max-samples=20000
done
```

Expected for each: `[ok] No violation found`, `max=2` in the trace-length statistics, and **every one of the four witnesses reached in more than 0 traces**.

A witness at `0 trace(s) ... (0.00%)` means that state is unreachable, so the `[ok]` beside it is vacuous — stop and fix the encoding. `wDiverged` in particular must fire: it is the verified L4 counterexample, and a model that cannot reach it has no adversarial power.

Then the exhaustive check. Each takes roughly 2.5 minutes at n=4/f=1; do not interrupt them:

```bash
quint verify broadcast.qnt --invariant=L1_NoFalseBlame --max-steps=1
quint verify broadcast.qnt --invariant=L2_ValueAgreement --max-steps=1
```

Expected: `The outcome is: NoError` and `[ok] No violation found` for both. Reference timing from the prototype under this encoding: L1 about 111 s.

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
    assert(PARTIES.forall(k => match verify(k, ALL_PRESENT, cs) {
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
  // satisfy the contract keygen.qnt assumes. `out` is broadcast.qnt's state
  // variable, already computed from the corrected `verify(k, present, claims)`.
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
- Produces: `type Stage`, `type CeremonyOutcome = Running | Done(Party -> Vote) | Failed(Set[Party])`, state vars `stage: Party -> Stage`, `result: Party -> CeremonyOutcome`; `pure val KEY_A/KEY_B`; invariants `K1_NoHonestBlamed`, `K2_NoConflictingOutcome`, `K5_Termination`.

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

  // Two distinct candidate keys, so K2 (no two honest parties finish with
  // different keys) is not vacuously true.
  pure val KEY_A: Party -> Vote = PARTIES.mapBy(i => Sent(i))
  pure val KEY_B: Party -> Vote = PARTIES.mapBy(i => Sent(i + 1))

  // One stage transition.
  //
  // Each party gets its OWN oracle result: the verified L4 counterexample shows
  // a Byzantine party can equivocate in round 1 and tie-break in round 2, so
  // some honest parties agree while others fail. A single shared result would
  // make K2 vacuous.
  //
  // CRITICAL: every choice is decoded into a valid configuration. Do NOT draw
  // freely and add constraints inside `all { }` - that disables the transition
  // rather than restricting the choice, and the cost of completing a trace then
  // grows about 2.6x per step (measured: 84s at 8 steps, over 9 minutes at 10).
  // The oracle contract is honoured structurally instead:
  //   - ONE agreed key is chosen per step, so two honest parties can never land
  //     on different agreed maps (this is L2, which broadcast.qnt proved);
  //   - blame is always BYZ - non-empty and disjoint from HONEST (this is L1);
  //   - agreement is only offered while a quorum is still participating, because
  //     every stage collects from all participants and the rest time out once
  //     too many have aborted.
  action step = {
    nondet keyIsA = Set(false, true).oneOf()
    nondet agreedSet = PARTIES.powerset().oneOf()
    nondet blamedSet = PARTIES.powerset().oneOf()

    val stillRunning = PARTIES.filter(p => result.get(p) == Running)
    val canAgree = stillRunning.size() > T
    val theKey = if (keyIsA) KEY_A else KEY_B
    val res = PARTIES.mapBy(k =>
      if (canAgree and agreedSet.contains(k)) OkAgreed(theKey)
      else if (blamedSet.contains(k)) OkAttributed(BYZ)
      else OkFailed)

    all {
      stage' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) stage.get(k)
        else if (isVerifyStage(stage.get(k)) and
                 (match res.get(k) {
                  | OkAgreed(_) => false
                  | OkAttributed(_) => true
                  | OkFailed => true
                 })) stage.get(k)
        else nextStage(stage.get(k))),
      result' = PARTIES.mapBy(k =>
        if (result.get(k) != Running) result.get(k)
        else if (isVerifyStage(stage.get(k)))
          match res.get(k) {
          | OkAgreed(m) => if (nextStage(stage.get(k)) == Finished) Done(m) else Running
          | OkAttributed(bad) => Failed(bad)
          | OkFailed => Failed(Set())
          }
        else if (nextStage(stage.get(k)) == Finished) Done(KEY_A)
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
  quint run keygen.qnt --invariant=$inv \
    --witnesses wCeremonyDiverged wCeremonyDone wCeremonyBlamed \
    --max-steps=12 --max-samples=20000
done
```

Expected: `TYPECHECK-OK`, `[ok]` for all three, `max=13` in the trace-length statistics, and every witness above 0%. `--max-steps=12` covers the ten stages plus slack.

**Reference timings under the structural encoding: about 130 ms for 2000 traces at 12 steps.** If a run instead takes minutes, a rejection guard has crept into `step` — find it and decode it into the draw instead. Reference witness coverage: diverged ~97%, blamed ~87%, done ~0.5%.

`wCeremonyDone` is deliberately low: reaching `Done` needs agreement at all four verify stages. That means K2 and K6, which only bite on `Done`, are thinly exercised by simulation — which is why K1 is also checked exhaustively:

```bash
quint verify keygen.qnt --invariant=K1_NoHonestBlamed --max-steps=12
```

Expected: `The outcome is: NoError`, about 50 s.

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
- Produces: state vars `shareValid: Set[{ from: Party, to: Party }]`, `complaints: Set[{ by: Party, about: Party }]`, `revealed: Set[{ by: Party, about: Party, ok: bool }]`; `pure def blameResponseComplete(...)`; invariant `K3_AttributionProgress`. All three state vars must be declared, initialised in `init`, and assigned in every `step` branch — Quint requires every transition to assign every variable.

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
      revealed' = if (PARTIES.exists(k => stage.get(k) == BlameResponses8))
                    honestReveals.union(byzReveals) else revealed,
```

with the `nondet` picks hoisted to the top of `step` alongside the existing `keyIsA` / `agreedSet` / `blamedSet` picks (no guards — every draw must decode to a valid configuration):

```quint
    nondet byzBadShares =
      tuples(BYZ, PARTIES).map(t => { from: t._1, to: t._2 }).powerset().oneOf()
    nondet byzComplaints =
      tuples(BYZ, PARTIES).map(t => { by: t._1, about: t._2 }).powerset().oneOf()
    val honestComplaints =
      tuples(HONEST, PARTIES)
        .filter(t => not(shareValid.contains({ from: t._2, to: t._1 })))
        .map(t => { by: t._1, about: t._2 })
    // Stage 8: a blamed party reveals the share it sent to each complainant.
    // An honest party reveals exactly those, truthfully. A Byzantine party may
    // reveal any subset, with any validity.
    // Revealed-at-all and valid-or-not, as two characteristic subsets, so a
    // draw can never assert both ok:true and ok:false for the same pair.
    nondet byzRevealMade = tuples(BYZ, PARTIES).powerset().oneOf()
    nondet byzRevealOk = tuples(BYZ, PARTIES).powerset().oneOf()
    val byzReveals = byzRevealMade.map(t =>
      { by: t._1, about: t._2, ok: byzRevealOk.contains(t) })
    val honestReveals =
      complaints.filter(c => HONEST.contains(c.about))
        .map(c => { by: c.about, about: c.by,
                    ok: shareValid.contains({ from: c.about, to: c.by }) })
```

All three picks are powersets of record sets — never `setOfMaps`. Delete the standalone `chooseShares` / `chooseComplaints` actions shown above; they are inlined here.

`revealed` must also be declared and initialised alongside the others:

```quint
  var revealed: Set[{ by: Party, about: Party, ok: bool }]
```

with `revealed' = Set(),` added to `init`. Task 12 consumes `revealed` and will not compile without it.

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
  // `threshold + 1` coefficients. Declared `const` so harness.qnt can
  // instantiate a deliberately-broken variant as a permanent negative control,
  // rather than this being a manual edit-and-revert that nobody re-runs.
  const ENFORCE_COEFF_LENGTH: bool

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

  // 0 and KEY_THRESHOLD are too short; KEY_THRESHOLD + 2 is too long;
  // KEY_THRESHOLD + 1 is the honest length.
```

In `init`: `coeffLen' = PARTIES.mapBy(_ => KEY_THRESHOLD + 1),`

In `step`, hoisted alongside the other picks:

```quint
    // Four length choices per Byzantine party via two characteristic subsets.
    nondet lenHi = BYZ.powerset().oneOf()
    nondet lenLo = BYZ.powerset().oneOf()
    val newLens = PARTIES.mapBy(i =>
      if (not(BYZ.contains(i))) KEY_THRESHOLD + 1
      else if (lenHi.contains(i))
        (if (lenLo.contains(i)) 0 else KEY_THRESHOLD)
      else
        (if (lenLo.contains(i)) KEY_THRESHOLD + 1 else KEY_THRESHOLD + 2))
```

with this assignment:

```quint
      coeffLen' = if (PARTIES.exists(k => stage.get(k) == CoefficientCommitments3))
                    newLens else coeffLen,
```

and the `VerifyCommitments4` branch failing with the offending parties attributed:

```quint
    // Checked against the COMMITTED lengths in `coeffLen`, NOT against this
    // step's fresh `newLens` draw. The commitment is broadcast at stage 3 and
    // validated at stage 4, one step later. Reading `newLens` here tests a
    // value that was never committed, and K6 then fails even with the check
    // enabled - a trap this plan hit once already.
    val badLenParties = PARTIES.filter(i => not(commitmentAccepted(coeffLen.get(i))))
```

Both the `stage'` and `result'` branches must consult it, so a party at `VerifyCommitments4` with any bad-length commitment in play fails and stops rather than advancing:

```quint
        else if (stage.get(k) == VerifyCommitments4 and badLenParties != Set())
          Failed(badLenParties)
```

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

Because `ENFORCE_COEFF_LENGTH` is a `const`, `keygen.qnt` cannot be run directly — it must be instantiated. Create a throwaway probe to confirm both polarities, then delete it (Task 12 makes the negative control permanent in `harness.qnt`):

```quint
module coeffprobe {
  import keygen(ENFORCE_COEFF_LENGTH = true, SHARING = PARTIES, RECEIVING = PARTIES).* from "./keygen"
}
```

```bash
quint run coeffprobe.qnt --invariant=K6_KeyConsistency --max-steps=12 --max-samples=20000 \
  | grep -E '^\[(ok|violation)\]'
```

Expected: `[ok]` with `max=13`.

If this reports `[violation]` even with the check enabled, the wiring is reading `newLens` instead of `coeffLen` — see the timing note above.

Then flip the instantiation to `ENFORCE_COEFF_LENGTH = false` and re-run:

Expected: `[violation] Found an issue`, with a trace where a Byzantine party commits to `KEY_THRESHOLD + 2` coefficients and the ceremony still reaches `Done`. That is finding #1 reproduced in the model.

**If this still reports `[ok]`, the model is too weak to have found the bug the review identified** — fix it before continuing.

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
- Consumes: Tasks 9-11's model.
- Produces: in `keygen.qnt`, `const SHARING`, `const RECEIVING`, `const FILTER_NON_RECEIVER_COMPLAINTS`, `const ENFORCE_COEFF_LENGTH`, `pure def admissibleComplaints`; in `harness.qnt`, modules `plain`, `handover`, `handoverUnfixed`, `plainNoCoeffCheck` with `K4_HandoverNoFalseBlame`, the witnesses `wCeremonyDiverged` / `wCeremonyDone` / `wCeremonyBlamed`, and the negative controls `K4_MustFailHere` / `K6_MustFailHere`.

Note: introducing `const`s means `keygen.qnt` can no longer be run directly — every check must target an instantiating module in `harness.qnt`. Task 13 updates `check.sh` accordingly.

Handover splits parties into sharers and receivers with an index remapping. `VerifyComplaintsBroadcastStage7` already discards complaints from non-receiving participants, because acting on one forces the blamed party to reveal a share at a bogus index, fail stage-9 verification, and be wrongly attributed. **The model must not bake that fix in as an assumption** — it should be able to express the unfixed behaviour, so K4 demonstrably fails without the filter and passes with it.

- [ ] **Step 1: Write the failing invariant**

Create `harness.qnt`:

Each configuration is a separate module instantiating `keygen` with different
`const` values. A module may instantiate `keygen` only once, so each
configuration gets its own file-level module in `harness.qnt`.

```quint
// Handover: n=5, f=1, sharers {1,2,3}, receivers {3,4,5}. Deliberately tight -
// a quorum over three sharers is two, so there is no slack if the Byzantine
// party is a sharer.
module handover {
  import types.* from "./types"
  import keygen(
    SHARING = Set(1, 2, 3),
    RECEIVING = Set(3, 4, 5),
    FILTER_NON_RECEIVER_COMPLAINTS = true,
    ENFORCE_COEFF_LENGTH = true
  ).* from "./keygen"

  // K4 is K1 under the handover configuration - the property is the same, the
  // configuration is what makes it a distinct check.
  val K4_HandoverNoFalseBlame = K1_NoHonestBlamed

  // K7 is a WITNESS, not an invariant. Stating it as "no honest party may
  // finalise while another failed" is false of the real protocol: a minority of
  // honest parties aborting while the quorum completes is correct threshold
  // behaviour. Whether a locally-finalised key is ever used is decided outside
  // this model, by the State Chain requiring a threshold of success reports.
  // What the model must show is that it can still REACH divergence - a model
  // that cannot has lost the adversarial power the L4 counterexample proved is
  // real. Must be witnessed in > 0 traces.
  val wCeremonyDiverged = tuples(HONEST, HONEST).exists(q =>
    result.get(q._1) != result.get(q._2))
  val wCeremonyDone = HONEST.exists(k =>
    match result.get(k) { Done(_) => true | Failed(_) => false | Running => false })
  val wCeremonyBlamed = HONEST.exists(k =>
    match result.get(k) { Failed(bad) => bad != Set() | Done(_) => false | Running => false })
}

// Negative control: the non-receiver complaint filter switched OFF. K4 MUST
// fail here. If it passes, the model cannot see a bug the Rust already fixes
// (VerifyComplaintsBroadcastStage7) and is not trustworthy for finding others.
module handoverUnfixed {
  import types.* from "./types"
  import keygen(
    SHARING = Set(1, 2, 3),
    RECEIVING = Set(3, 4, 5),
    FILTER_NON_RECEIVER_COMPLAINTS = false,
    ENFORCE_COEFF_LENGTH = true
  ).* from "./keygen"

  val K4_MustFailHere = K1_NoHonestBlamed
}

// Negative control: the coefficient-length check switched OFF. K6 MUST fail
// here - this is review finding #1 reproduced.
module plainNoCoeffCheck {
  import types.* from "./types"
  import keygen(
    SHARING = PARTIES,
    RECEIVING = PARTIES,
    FILTER_NON_RECEIVER_COMPLAINTS = true,
    ENFORCE_COEFF_LENGTH = false
  ).* from "./keygen"

  val K6_MustFailHere = K6_KeyConsistency
}
```

Plain keygen (`SHARING = RECEIVING = PARTIES`, both flags true) also needs a
module, since Tasks 9-11 checked `keygen.qnt` directly and it now has `const`s:

```quint
module plain {
  import types.* from "./types"
  import keygen(
    SHARING = PARTIES,
    RECEIVING = PARTIES,
    FILTER_NON_RECEIVER_COMPLAINTS = true,
    ENFORCE_COEFF_LENGTH = true
  ).* from "./keygen"
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
  // `const` so harness.qnt can instantiate the unfixed variant as a permanent
  // negative control.
  const FILTER_NON_RECEIVER_COMPLAINTS: bool

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

and `SHARING` / `RECEIVING` are declared as `const` in `keygen.qnt`, supplied per-instantiation by `harness.qnt`. Plain keygen is the instantiation where both equal `PARTIES`:

```quint
  // Supplied per-instantiation. Plain keygen: SHARING = RECEIVING = PARTIES.
  const SHARING: Set[Party]
  const RECEIVING: Set[Party]
```

Quint `const` + parameterised instancing is confirmed working on 0.32.0: `import keygen(SHARING = Set(1,2,3), ...).* from "./keygen"` typechecks, and the importing module inherits `keygen`'s `init`/`step`, so `quint run harness.qnt --main=handover` drives the instantiated machine. Several instantiations may live in one file.

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

The negative controls are permanent modules, so this is a normal run — no editing and reverting:

```bash
quint run harness.qnt --main=handoverUnfixed --invariant=K4_MustFailHere \
  --max-steps=12 --max-samples=50000 | grep -E '^\[(ok|violation)\]'
quint run harness.qnt --main=plainNoCoeffCheck --invariant=K6_MustFailHere \
  --max-steps=12 --max-samples=50000 | grep -E '^\[(ok|violation)\]'
```

Expected: `[violation] Found an issue` for BOTH. The first trace should show a non-receiving Byzantine party complaining about an honest sharer, which then lands in a reported set. The second should show a Byzantine party committing to `KEY_THRESHOLD + 2` coefficients with the ceremony still reaching `Done`.

**If either reports `[ok]`, the model is too weak** — it cannot express a bug the Rust already fixes, so it cannot be trusted to find similar ones. Fix the model before continuing.

`--main=<module>` is confirmed working on quint 0.32.0, as is holding several instantiations of the same `const`-parameterised module in one file. Without `--main`, quint infers the module from the filename, which is wrong for a multi-module `harness.qnt` — so every harness check must pass `--main` explicitly.

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
