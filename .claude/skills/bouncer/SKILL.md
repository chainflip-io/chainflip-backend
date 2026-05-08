---
name: bouncer
description: Use for anything involving the chainflip bouncer or localnet — running end-to-end tests, starting/stopping a localnet, getting a localnet ready for testing, running setup scripts, or regenerating the bouncer's event schemas. Triggered by phrases like "run the bouncer test", "test this in the bouncer", "kick off LpApi", "run the lending test", "spin up a localnet", "start a localnet", "get a localnet ready", "tear down the localnet", "regenerate the schemas", "regenerate event types", or naming a specific bouncer test. Covers localnet lifecycle, test setup scripts, schema regeneration, running individual tests, finding test names, debugging logs, and pre-commit checks.
---

# Running bouncer tests

The bouncer is a TypeScript end-to-end test suite at `bouncer/`. It runs against a local Chainflip network (state chain node + engine + chainflip-broker-api + chainflip-lp-api + simulated external chains) booted by scripts in `./localnet/`.

## TL;DR

```bash
# Build, recreate localnet, run setup
./localnet/build_and_run.sh

# Run a test
cd bouncer && pnpm vitest run -t "LpApiLending"
```

`build_and_run.sh` does `cargo build && ./localnet/recreate.sh -d && (cd bouncer && ./setup_for_test.sh)`. Event schemas are regenerated as part of the boot — see Section 5.

One-time per checkout: `cd bouncer && pnpm install`.

## 1. Liveness and version check

Before doing anything, find out what state you're in.

### Is a localnet running?

```bash
curl -s -X POST -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"system_chain","params":[]}' \
  http://127.0.0.1:9944
```

`{"jsonrpc":"2.0","id":1,"result":"CF Develop"}` → up. Connection refused / nothing → not running.

### Is the running localnet (and binary) on the current git HEAD?

From `bouncer/`:

```bash
./commands/check_localnet_commit.ts
```

Compares `system_version` RPC and `./target/debug/chainflip-node -V` against `git rev-parse HEAD`. Exits 0 if both match, 1 if either is stale or unreachable. If stale, see Section 2.

## 2. Starting a localnet

| Want                                                 | Script                                 |
| ---------------------------------------------------- | -------------------------------------- |
| Build, recreate, and run setup (default)             | `./localnet/build_and_run.sh`          |
| Reset chain state with current binaries (no rebuild) | `./localnet/recreate.sh -d`            |
| First-ever boot, log tailing, or non-default options | `./localnet/manage.sh` (interactive)   |
| Tear down only                                       | `./localnet/manage.sh` → `destroy`     |
| Localnet up and only test code changed               | Skip — just `pnpm vitest run -t "..."` |

**Always confirm with the user before running anything that destroys an existing localnet** (`build_and_run.sh`, `recreate.sh`, `destroy`).

`recreate.sh` reuses settings (node count, binary path) from `/tmp/chainflip/settings.sh`, saved by a prior `manage.sh build-localnet`. `-d` falls back to defaults (`./target/debug`, 1 node) when no settings file exists. `manage.sh` interactive options: `1` build-localnet, `2` recreate, `3` destroy, `4` logs.

A fresh boot starts everything: state chain node, engine, chainflip-broker-api, chainflip-lp-api, deposit-monitor, indexer, btc/eth/sol/dot simulators.

## 3. Setup scripts

`./bouncer/setup_for_test.sh` runs `commands/setup_vaults.ts` then `commands/setup_concurrent.ts`:

- **`setup_vaults.ts`** — initialises Polkadot/Arbitrum/Solana chains, forces a validator rotation, registers the new vault keys with the state chain, sets up price feeds. Equivalent to "make TSS happen for external chains".
- **`setup_concurrent.ts`** — creates swap pools, range orders (zero-to-infinity), boost pools, lending pools, witnessing config. Without it most tests fail with "no swap pool" / "no boost pool" errors.

`build_and_run.sh` runs this for you. Only invoke manually if you booted via `recreate.sh` or `manage.sh` directly.

### Has setup already run? Check before re-running.

`setup_for_test.sh` is **not idempotent** — `setup_vaults.ts` calls `validator.forceRotation()` unconditionally; `setup_concurrent.ts` emits `governance:FailedExecution` when target objects already exist.

The most reliable signal is the BTC lending pool — it doesn't exist at genesis, only after `setup_concurrent.ts`:

```bash
cd bouncer
pnpm tsx -e "
import { getChainflipApi } from 'shared/utils/substrate';
const api = await getChainflipApi();
const btcPool = (await api.query.lendingPools.generalLendingPools('Btc')).toJSON();
console.log(btcPool === null ? 'NOT_SET_UP' : 'READY');
process.exit(0);
"
```

`cf_available_pools` is **not** a reliable signal — the dev chainspec pre-populates swap pools at genesis, so it returns 11 even before setup.

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
```

`run_test.ts` is the most ergonomic single-test runner: it sets `BOUNCER_LOG_LEVEL=debug` and resolves the test name by matching the exported function name against `pnpm vitest list`.

Test names are the first arg to `concurrentTest(...)` / `serialTest(...)` in `bouncer/tests/fast_bouncer.test.ts` and `bouncer/tests/full_bouncer.test.ts` — not the function name. `pnpm vitest list` prints them all.

### Multiple / all tests

```bash
# All concurrent tests
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
