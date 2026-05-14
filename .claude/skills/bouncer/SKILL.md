---
name: bouncer
description: Use for Chainflip bouncer or localnet tasks: run end-to-end tests, start or rebuild localnet, run bouncer setup scripts, regenerate event schemas, debug bouncer logs, and run pre-commit TypeScript checks. Trigger on requests like "run the bouncer test", "run the fast bouncer tests", "start a localnet", "rebuild the localnet", "regenerate schemas", "run bouncer lints", or when a specific bouncer test is named.
---

# Running bouncer tests

The bouncer is a TypeScript end-to-end test suite at `bouncer/`. It runs against a local Chainflip network (state chain node + engine + chainflip-broker-api + chainflip-lp-api + simulated external chains) booted by scripts in `./localnet/`.

> ⚠️ **STOP — destructive command rule.** Before running **any** command that tears down or recreates a localnet — `./localnet/build_and_run.sh`, `./localnet/recreate.sh` (with or without `-d`), `./localnet/manage.sh` with `destroy`/`recreate`/`build-localnet`, `./fast_bouncer.sh`, `./full_bouncer.sh`, or anything else that wipes `/tmp/chainflip` — you **must** ask the user explicitly and wait for confirmation. Once the user has confirmed a destructive command in this session (until the terminal is closed), further destructive commands in the same session don't need a fresh prompt. Running tests (`pnpm vitest …`, `./commands/run_test.ts`, `./bouncer/setup_for_test.sh`, schema regeneration) against an existing localnet is fine without prompting.

## TL;DR

```bash
# Build, recreate localnet, run setup
./localnet/build_and_run.sh

# Run a test
cd bouncer && pnpm vitest run -t "LpApiLending"
```

`build_and_run.sh` does `cargo build && ./localnet/recreate.sh -d && (cd bouncer && ./setup_for_test.sh)`. Event schemas are regenerated as part of the boot — see Section 5.

> 🚨 **Check for rtk once at the start of the session, then wrap every vitest call.** Run `command -v rtk` once. If it prints a path, the user has the rtk shell hook installed and bare `pnpm vitest …` will have its stdout mangled — you'll see `PASS (0) FAIL (0)` and exit 1 even when nothing is wrong. **If rtk is present**, prefix `rtk proxy ` to every `pnpm vitest …` invocation in this skill, including the shell scripts that wrap it (`./commands/run_test.ts`, `./fast_bouncer.sh`, `./full_bouncer.sh`) — e.g. `rtk proxy pnpm vitest run -t "..."`, `rtk proxy ./commands/run_test.ts …`. **If rtk is not present**, run the commands as written. Examples below show the bare `pnpm vitest …` form; add `rtk proxy` in front when rtk is installed.

> 🛠️ **Run `pnpm install` in `bouncer/` before booting or setting up a localnet** — i.e. before `build_and_run.sh`, `recreate.sh`, `setup_for_test.sh`, or schema regeneration. Dependencies drift between branches and stale `node_modules` cause confusing failures. **Skip it if the localnet is already running and setup has completed** (Section 1 / `./commands/check_setup_complete.ts`); just `pnpm vitest …` against an existing setup doesn't need a reinstall. If a test fails to resolve imports, fall back to `pnpm install` then retry.

## 1. Liveness and version check

Before doing anything, find out what state you're in. **Run all three checks in parallel** (single message, three Bash calls) — they're independent and fast:

1. **Liveness** — is anything answering on the state-chain RPC port?

   ```bash
   curl -s -X POST -H 'Content-Type: application/json' \
     -d '{"jsonrpc":"2.0","id":1,"method":"system_chain","params":[]}' \
     http://127.0.0.1:9944
   ```

   `{"jsonrpc":"2.0","id":1,"result":"CF Develop"}` → up. Connection refused / nothing → not running.

2. **Version match** — is the running localnet on the current git HEAD? From `bouncer/`:

   ```bash
   ./commands/check_localnet_commit.ts
   ```

   Compares the `system_version` RPC against `git rev-parse HEAD`. Exits 0 if it matches, 1 if stale or unreachable. If stale, see Section 2.

3. **Setup status** — has `setup_for_test.sh` already run against this localnet? Run `./commands/check_setup_complete.ts` from `bouncer/` (Section 3).

If all three pass, skip straight to Section 4 and run the test.

## 2. Starting a localnet

| Want                                                 | Script                                 |
| ---------------------------------------------------- | -------------------------------------- |
| Build, recreate, and run setup (default)             | `./localnet/build_and_run.sh`          |
| Reset chain state with current binaries (no rebuild) | `./localnet/recreate.sh -d`            |
| First-ever boot, log tailing, or non-default options | `./localnet/manage.sh` (interactive)   |
| Tear down only                                       | `./localnet/manage.sh` → `destroy`     |
| Localnet up and only test code changed               | Skip — just `pnpm vitest run -t "..."` |

**Destructive command rule (repeat from top):** `build_and_run.sh`, `recreate.sh`, and `manage.sh` `destroy`/`recreate`/`build-localnet` all wipe the running localnet. Ask the user before the first one in a session; once they've confirmed, subsequent destructive commands in the same session don't need a fresh prompt.

If localnet startup fails or services crash, check `/tmp/chainflip/*.log` first. If the failure looks like stale or partial state, run `./localnet/recreate.sh -d` and retry.

`recreate.sh` reuses settings (node count, binary path) from `/tmp/chainflip/settings.sh`, saved by a prior `manage.sh build-localnet`. `-d` falls back to defaults (`./target/debug`, 1 node) when no settings file exists. `manage.sh` interactive options: `1` build-localnet, `2` recreate, `3` destroy, `4` logs.

A fresh boot starts everything: state chain node, engine, chainflip-broker-api, chainflip-lp-api, deposit-monitor, indexer, btc/eth/sol/dot simulators.

## 3. Setup scripts

`./bouncer/setup_for_test.sh` runs `commands/setup_vaults.ts` then `commands/setup_concurrent.ts`:

- **`setup_vaults.ts`** — initialises Polkadot/Arbitrum/Solana chains, forces a validator rotation, registers the new vault keys with the state chain, sets up price feeds. Equivalent to "make TSS happen for external chains".
- **`setup_concurrent.ts`** — creates swap pools, range orders (zero-to-infinity), boost pools, lending pools, witnessing config. Without it most tests fail with "no swap pool" / "no boost pool" errors.

`build_and_run.sh` runs this for you. Only invoke manually if you booted via `recreate.sh` or `manage.sh` directly.

If `setup_for_test.sh` fails, **run `cd bouncer && pnpm install` first** before investigating anything else — stale dependencies are the most common cause. (Not needed if you've already installed for this checkout and the localnet has been running fine.)

### Has setup already run? Use this single check before re-running.

`setup_for_test.sh` is **not idempotent** — `setup_vaults.ts` calls `validator.forceRotation()` unconditionally; `setup_concurrent.ts` emits `governance:FailedExecution` when target objects already exist.

From `bouncer/`:

```bash
./commands/check_setup_complete.ts
```

Exits 0 (`READY`) if the BTC lending pool exists — a marker that `setup_concurrent.ts` ran — and 1 (`NOT_SET_UP`) otherwise.

If setup is partially done (e.g. `setup_concurrent.ts` crashed mid-way), recreate the localnet rather than re-running setup — `forceRotation` can't cleanly re-run.

## 4. Running tests

### A single test

From `bouncer/`:

```bash
# By test name
pnpm vitest run -t "LpApiLending"

# Debug-level stdout (TRACE still goes to the log file)
BOUNCER_LOG_LEVEL=debug pnpm vitest run -t "LpApiLending"

# By test file (auto-resolves the name)
./commands/run_test.ts ./tests/lp_api_lending_test.ts

# By swap number (re-run a single AllSwaps case, e.g. after a flake)
./commands/run_test.ts 318
```

`run_test.ts` is the most ergonomic single-test runner: it sets `BOUNCER_LOG_LEVEL=debug` and accepts either a test file path (resolves the test name by matching the exported function name against `pnpm vitest list`) or a bare integer (runs `Swap <N>:` from `tests/fast_bouncer.test.ts`). It does **not** take `-t` or any other flags — just the one positional arg.

Test names are the first arg to `concurrentTest(...)` / `serialTest(...)` in `bouncer/tests/fast_bouncer.test.ts` and `bouncer/tests/full_bouncer.test.ts` — not the function name. To find one:

```bash
# From bouncer/. Writes /tmp/chainflip/test_info.csv with `TestName,functionName` rows.
pnpm vitest list >/dev/null
grep -i "<keyword>" /tmp/chainflip/test_info.csv
```

The CSV is the cleanest source — one row per test, both the runnable test name and the exported function name. `pnpm vitest list` on its own prints to stdout too, but the CSV is easier to grep.

If the first grep returns a single clean row, that's your test — don't re-grep with a broader pattern to "make sure." The CSV is exhaustive; a unique hit is unique.

### Multiple / all tests

```bash
# All concurrent tests. This is the default when asked to "run the bouncer" without a specific test name/group.
pnpm vitest --maxConcurrency=100 run -t "ConcurrentTests"

# Everything in a file
pnpm vitest --maxConcurrency=100 run tests/fast_bouncer.test.ts

# Full fast-bouncer including setup_for_test.sh (assumes localnet is up)
./fast_bouncer.sh

# Full bouncer (used by ci-main-merge)
./full_bouncer.sh 1-node
```

`fast_bouncer.sh` and `full_bouncer.sh` already include `setup_for_test.sh`.

**`AllSwaps` and `ConcurrentTests` are `describe` blocks, not single tests.** They fan out to many per-asset tests via `testAllSwaps()`. Always pass `--maxConcurrency=100`, otherwise vitest's default of 5 makes them take forever:

```bash
pnpm vitest --maxConcurrency=100 run -t "AllSwaps"
```

### Long runs: background + Monitor

Rough durations on a healthy 1-node localnet:

| Run                                   | Wall time  |
| ------------------------------------- | ---------- |
| Single test (e.g. `LpApi`)            | 1–5 min    |
| `AllSwaps` describe block             | ~10 min    |
| `ConcurrentTests` (default "bouncer") | ~15–20 min |
| `./fast_bouncer.sh`                   | ~25 min    |
| `./full_bouncer.sh 1-node`            | 40 min+    |

Anything `AllSwaps` or larger: **always run in the background** and tee to a log file. Foreground tool calls cap at 10 minutes, and even within that the lack of streamed output makes debugging painful.

Pattern:

```bash
# Background run, all output to a file
pnpm vitest --maxConcurrency=100 run -t "ConcurrentTests" > /tmp/chainflip/bouncer_run.log 2>&1
# (launch with run_in_background: true)
```

Attach a `Monitor` to stream just the signal you care about — don't tail the whole log, the volume is large:

```bash
tail -f /tmp/chainflip/bouncer_run.log | grep -E --line-buffered "✓|✗|FAIL|Test Files |Tests |Duration|Error"
```

When the background task completes, the run summary is at the bottom of the log file. Extract it directly:

```bash
grep -E "Test Files|^ +Tests |Duration|^ FAIL " /tmp/chainflip/bouncer_run.log | tail -20
```

You'll get something like:

```
 Test Files  1 failed | 2 skipped (3)
      Tests  1 failed | 659 passed | 677 skipped (1337)
   Duration  1096.58s
 FAIL  tests/fast_bouncer.test.ts > ConcurrentTests > AllSwaps > Swap 318: Sol to SolUsdt (CCM VaultSwap)
```

That's enough to report the result without re-reading the full log. To re-run a single failed `AllSwaps` case, use `./commands/run_test.ts <swap_number>` (see "A single test" above).

## 5. Regenerating event schemas

`bouncer/generated/events/` contains zod schemas auto-generated from the runtime metadata. **Schemas are regenerated automatically on every localnet boot** — `localnet/common.sh`'s `build-localnet` invokes `generate_event_schemas.ts` after starting the LP API. `build_and_run.sh`, `recreate.sh`, and `manage.sh build-localnet` all produce fresh schemas.

Run the generator manually only if:

- You changed a pallet event and want fresh schemas without recreating the localnet, or
- You suspect committed schemas are stale relative to a running localnet.

```bash
cd bouncer
./commands/generate_event_schemas.ts   # requires a running localnet
```

The generator deletes `bouncer/generated/events/` and rewrites it. After regeneration, run `pnpm prettier:write` and commit the diff alongside your pallet changes. Tests importing schemas that no longer exist (e.g. event renamed) need to be updated.

## 6. Debugging a failure

The TRACE-level log file is the source of truth — `stdout` is filtered.

```bash
# Per-test slice from the bouncer log
jq 'select(.test=="LpApiLending")' /tmp/chainflip/bouncer.log > /tmp/chainflip/lpapilending.log

# Just errors for a test
jq 'select(.test=="LpApiLending" and .level >= 50)' /tmp/chainflip/bouncer.log
```

Pino levels: 30=info, 40=warn, 50=error.

`BOUNCER_LOG_PATH=/tmp/foo.log` redirects the log file. `BOUNCER_LOG_LEVEL` only affects stdout.

For chain-side issues (engine, state chain), look in `/tmp/chainflip/<service>.log` (`chainflip-node.log`, `chainflip-engine.log`, `chainflip-lp-api.log`, `chainflip-broker-api.log`).

## 7. Pre-commit checks

CI fails on any of these. Run from `bouncer/`:

```bash
pnpm tsc --noEmit          # Type-check
pnpm eslint:check          # Lint
pnpm prettier:check        # Format

# Auto-fix variants
pnpm eslint:fix
pnpm prettier:write
```

## When _not_ to use the bouncer

- Pallet-level changes with no cross-component effect → `cargo nextest run -p <pallet>`.
- Multi-pallet runtime interactions that don't need external chains → `cf-integration-tests` (`cargo nextest run -p cf-integration-tests`).
- Anything that doesn't touch the engine, an external chain, or the LP/broker API server → bouncer is overkill and slow.

Reach for the bouncer when the change touches end-to-end flows: the engine, witnessing, threshold signing, broadcasts, the LP/broker JSON-RPC servers, or anything that depends on real BTC/ETH/SOL deposits and broadcasts.
