---
name: bouncer
description: Use for Chainflip bouncer or localnet tasks: run end-to-end tests, start or rebuild localnet, run bouncer setup scripts, regenerate event schemas, debug bouncer logs, run pre-commit TypeScript checks, query state chain storage values, update pallet config items via governance, and set the runtime safe mode. Trigger on requests like "run the bouncer test", "run the fast bouncer tests", "start a localnet", "rebuild the localnet", "regenerate schemas", "run bouncer lints", a named bouncer test, "what is the value of X on the statechain/bouncer/mainnet", "query X storage", "change/update the bouncer X config", or "set/change the safe mode".
---

# Running bouncer tests

The bouncer is a TypeScript end-to-end test suite at `bouncer/`. It runs against a local Chainflip network (state chain node + engine + chainflip-broker-api + chainflip-lp-api + simulated external chains) booted by scripts in `./localnet/`.

> ⚠️ **STOP — destructive command rule.**
>
> Before any destructive command, run `cd bouncer && ./commands/check_localnet_state.ts` (Section 1) and use the reported `State`:
>
> - `DOWN` → nothing to destroy, proceed without prompting.
> - `READY` or `UNREADY` (running, on HEAD) → no confirmation needed.
> - `STALE` (running, commit ≠ HEAD) → ask the user explicitly and wait for confirmation before destroying.
>
> Full list of destructive commands: `./localnet/build_and_run.sh`, `./localnet/recreate.sh`, `./localnet/manage.sh`, `./fast_bouncer.sh`, `./full_bouncer.sh`.
> If a destructive command fails mid-way, check `/tmp/chainflip/*.log`, report the failure, and re-apply this rule before retrying.

## TL;DR

```bash
# Build, recreate localnet, run setup
./localnet/build_and_run.sh

# Run a test
cd bouncer && ./commands/run_test.ts BoostingForAsset
```

**Preflight — `pnpm install`.** Run `pnpm install` in `bouncer/` before booting or setting up a localnet (`build_and_run.sh`, `recreate.sh`, `setup_for_test.sh`, schema regeneration). Skip only if `check_localnet_state.ts` reports `State: READY` — running tests against a READY localnet doesn't need a reinstall. If a test fails to resolve imports, fall back to `pnpm install` and retry. If the install itself fails, log the error and display a message to the user explaining the failure.

## 1. Localnet state check

Before doing anything, find out what state you're in. One command covers liveness, commit match, and setup status:

```bash
cd bouncer && ./commands/check_localnet_state.ts
```

Prints `Liveness`, `Commit`, `Setup` lines, ending with `State: <STATE>`. Exits 0 only when `READY`.

| `State`   | Meaning                                              | Next command (cwd `bouncer/` unless noted)                                 |
| --------- | ---------------------------------------------------- | -------------------------------------------------------------------------- |
| `DOWN`    | No localnet reachable on 127.0.0.1:9944              | `./localnet/build_and_run.sh` (from repo root, no prompt). See §2.         |
| `STALE`   | Running, but commit hash ≠ current git HEAD          | Ask the user, then `./localnet/build_and_run.sh` (from repo root). See §2. |
| `UNREADY` | Running and on HEAD, but `setup_for_test.sh` not run | `./setup_for_test.sh`. See §3.                                             |
| `READY`   | Running, on HEAD, setup complete                     | Skip to §4 and run the test.                                               |

> **Note on `STALE`:** the commit hash is baked into the node binary by a build script that's cache-keyed on Rust source. A commit that changes only non-binary files (docs, `bouncer/**` TypeScript, `.github/**`) won't trigger a rebuild, so the binary keeps the _previous_ commit hash and the check reports `STALE` even though the running code is effectively current. If the only commits since the running hash are non-binary changes, a rebuild won't help and it's safe to proceed.

## 2. Starting a localnet

| Want                                                 | Script                                 |
| ---------------------------------------------------- | -------------------------------------- |
| Build, recreate, and run setup (default)             | `./localnet/build_and_run.sh`          |
| Reset chain state with current binaries (no rebuild) | `./localnet/recreate.sh -d`            |
| First-ever boot, log tailing, or non-default options | `./localnet/manage.sh` (interactive)   |
| Tear down only                                       | `./localnet/manage.sh` → `destroy`     |
| Localnet up and only test code changed               | Skip — just `pnpm vitest run -t "..."` |

`build_and_run.sh` does `cargo build && ./localnet/recreate.sh -d && (cd bouncer && ./setup_for_test.sh)`. Event schemas are regenerated as part of the boot — see Section 5.

**Destructive command rule (repeat from top):** `build_and_run.sh`, `recreate.sh`, and `manage.sh` `destroy`/`recreate`/`build-localnet` all wipe the running localnet. Use `check_localnet_state.ts` from §1: prompt the user only when `State: STALE`. `DOWN`/`READY`/`UNREADY` need no prompt.

If localnet startup/setup fails, check `/tmp/chainflip/debug.log` and `/tmp/chainflip/setup_for_test.log` first. If the failure looks like stale or partial state, run `./localnet/recreate.sh -d` and retry.

`recreate.sh` reuses settings (node count, binary path) from `/tmp/chainflip/settings.sh`, saved by a prior `manage.sh build-localnet`. `-d` falls back to defaults (`./target/debug`, 1 node) when no settings file exists. `manage.sh` interactive options: `1` build-localnet, `2` recreate, `3` destroy, `4` logs.

A fresh boot starts everything: state chain node, engine, chainflip-broker-api, chainflip-lp-api, deposit-monitor, indexer, btc/eth/sol/dot simulators.

## 3. Setup scripts

`./bouncer/setup_for_test.sh` runs `commands/setup_vaults.ts` then `commands/setup_concurrent.ts`:

- **`setup_vaults.ts`** — initialises Polkadot/Arbitrum/Solana chains, forces a validator rotation, registers the new vault keys with the state chain, sets up price feeds. Equivalent to "make TSS happen for external chains".
- **`setup_concurrent.ts`** — creates swap pools, range orders (zero-to-infinity), boost pools, lending pools, witnessing config. Without it most tests fail with "no swap pool" / "no boost pool" errors.

`build_and_run.sh` runs this for you. Only invoke manually if you booted via `recreate.sh` or `manage.sh` directly.

### Re-running setup

`setup_for_test.sh` is **not idempotent** — `setup_vaults.ts` calls `validator.forceRotation()` unconditionally; `setup_concurrent.ts` emits `governance:FailedExecution` when target objects already exist. Always check `check_localnet_state.ts` (§1) first: only run setup when `State: UNREADY`.

If setup is partially done (e.g. `setup_concurrent.ts` crashed mid-way), recreate the localnet rather than re-running setup — `forceRotation` can't cleanly re-run.

## 4. Running tests

### A single test

From `bouncer/`. **All `run_test.ts` invocations run at `BOUNCER_LOG_LEVEL=debug`** — bump to trace via the bare-vitest form when you need it.

```bash
# By test name
./commands/run_test.ts LpApi

# By test file, auto-resolves the name from the exported function
./commands/run_test.ts ./tests/boost.ts

# By swap number — re-run a single AllSwaps case
./commands/run_test.ts 318

# Trace-level stdout (bypasses run_test.ts)
BOUNCER_LOG_LEVEL=trace pnpm vitest run -t "BoostingForAsset"
```

`run_test.ts` takes one positional arg — a test name, a `./tests/...ts` path, or an integer — and forwards to `BOUNCER_LOG_LEVEL=debug pnpm vitest --maxConcurrency=100 --hideSkippedTests run …`. It does **not** accept `-t` or any other flags. Use bare `pnpm vitest run -t "..."` only when you need a flag combination `run_test.ts` doesn't cover.

### Finding a test name

Test names are the first arg to `concurrentTest(...)` / `serialTest(...)` in `bouncer/tests/fast_bouncer.test.ts` and `bouncer/tests/full_bouncer.test.ts` — not the function name. `pnpm vitest list` writes them to `/tmp/chainflip/test_info.csv` as `TestName,functionName` rows. **Always regenerate the CSV first** — it's stale otherwise (or absent on a fresh checkout):

```bash
# From bouncer/. Regenerate, then grep.
pnpm vitest list >/dev/null
grep -i "<keyword>" /tmp/chainflip/test_info.csv
```

A single hit is your test — don't re-grep with a broader pattern to "make sure," the CSV is exhaustive.

### Multiple / all tests

**When the user says "run the bouncer" without naming a test, the default is `ConcurrentTests`** — the top-level describe block in `fast_bouncer.test.ts` that fans out to every concurrent test. Reach for `./fast_bouncer.sh` / `./full_bouncer.sh` only when explicitly asked.

```bash
# Default "run the bouncer" — every concurrent test
pnpm vitest --maxConcurrency=100 run -t "ConcurrentTests"

# Everything in a file
pnpm vitest --maxConcurrency=100 run tests/fast_bouncer.test.ts

# Full fast-bouncer including setup_for_test.sh (assumes localnet state UNREADY)
./fast_bouncer.sh

# Full bouncer (used by ci-main-merge)
./full_bouncer.sh 1-node
```

`fast_bouncer.sh` and `full_bouncer.sh` already include `setup_for_test.sh`.

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

The TRACE-level log file is the source of truth — `stdout` is filtered. The bouncer test wrapper tags every log record with `test` (the test name) and `level` (pino numeric level), so jq queries work out of the box:

```bash
# Per-test slice from the bouncer log
jq 'select(.test=="BoostingForAsset")' /tmp/chainflip/bouncer.log > /tmp/chainflip/BoostingForAsset.log

# Just errors for a test
jq 'select(.test=="BoostingForAsset" and .level >= 50)' /tmp/chainflip/bouncer.log
```

Pino levels: 30=info, 40=warn, 50=error.

`BOUNCER_LOG_PATH=/tmp/foo.log` redirects the log file. `BOUNCER_LOG_LEVEL` only affects stdout.

For chain-side issues (engine, state chain), look in `/tmp/chainflip/<service>.log` (`chainflip-node.log`, `chainflip-engine.log`, `chainflip-lp-api.log`, `chainflip-broker-api.log`).

### Querying the indexer DB

The localnet boots a Postgres-backed Substrate event indexer (squid-sdk) as part of docker-compose. It's the easiest way to trace on-chain events after a test failure — every block, extrinsic, call, and event is queryable.

Connect (creds and DB name come from `bouncer/.env`):

```bash
psql "postgres://postgres:postgres@127.0.0.1:5432/squid_archive"
```

If `psql` isn't installed, ask the user before installing it (on macOS: `brew install libpq && brew link --force libpq`).

Key tables (full schema in `bouncer/prisma/schema.prisma`):

| Table       | Useful columns                                                     |
| ----------- | ------------------------------------------------------------------ |
| `event`     | `name` (e.g. `Swapping.SwapRequested`), `args` (JSONB), `block_id` |
| `extrinsic` | `hash`, `success`, `error`, `block_id`                             |
| `call`      | `name`, `args`, `success`, `error`, `extrinsic_id`                 |
| `block`     | `height`, `hash`, `timestamp`                                      |

`event.args` has a GIN index, so JSONB lookups are fast. Typical recipes:

```sql
-- All events for a swap, ordered chronologically
SELECT b.height, e.name, e.args
FROM event e JOIN block b ON e.block_id = b.id
WHERE e.args @> '{"swapRequestId":"42"}'::jsonb
ORDER BY b.height, e.index_in_block;

-- Find a VAULT swap by its deposit tx hash (vault origins carry origin.txId;
-- deposit-channel origins do NOT — correlate those by deposit address/channel instead)
SELECT e.name, e.args
FROM event e
WHERE e.name = 'Swapping.SwapRequested'
  AND e.args::text LIKE '%<tx_hash>%';

-- Most common event names in the last N blocks (sanity check)
SELECT name, COUNT(*) FROM event
WHERE block_id IN (SELECT id FROM block ORDER BY height DESC LIMIT 200)
GROUP BY name ORDER BY count DESC LIMIT 20;
```

### Swap lifecycle reference

A successful swap walks this event chain — linked by `swapRequestId` in `args`:

`Swapping.SwapRequested` → `Swapping.SwapScheduled` → `Swapping.SwapExecuted` → `Swapping.SwapEgressScheduled` → `<Chain>IngressEgress.CcmBroadcastRequested` or `BatchBroadcastRequested` → `<Chain>Broadcaster.BroadcastSuccess`

A missing link points at the failure stage. Common failure events to grep for:

- **`Swapping.SwapEgressIgnored` / `Swapping.RefundEgressIgnored`** — output too small to cover gas; swap got stuck post-execution.
- **Missing `BroadcastSuccess`** for an egress that was requested — signing or broadcast infrastructure problem (check engine logs and `<Chain>Broadcaster.*` events).
- **`<Chain>IngressEgress.DepositFailed`** — deposit rejected or not witnessed.

For **vault swaps**, the `SwapRequested` origin carries the deposit `txId`, so you can find the originating `swapRequestId` by searching `event.args` for the tx hash from the bouncer log. **Deposit-channel swaps** carry no tx hash in `SwapRequested` (only `deposit_address`, `channel_id`, `deposit_block_height`) — correlate those by deposit address or channel id.

### Format gotchas when correlating across sources

The same value can appear in different encodings in logs, RPC output, and the indexer:

- **EVM addresses**: lowercased in some logs, EIP-55 mixed-case in others. Compare with `lower()`.
- **Bitcoin addresses**: standard BECH32/etc. in user-facing logs vs. internal state-chain enum representation in events.
- **Bitcoin tx hashes**: appear in reversed byte order in some logs/DB fields. If a lookup misses, try the reverse.

## 7. Pre-commit checks

Run before every commit that touches `bouncer/`. Run in any order.

```bash
pnpm prettier:write        # Format (auto-fix)
pnpm tsc --noEmit          # Type-check
pnpm eslint:check          # Lint (use eslint:fix for auto-fix)
```

## 8. Bouncer commands

`bouncer/commands/` holds standalone CLI scripts — run them directly from `bouncer/` (e.g. `./commands/<name>.ts`). Each one has a header comment documenting its arguments. The ones this skill leans on:

| Command                          | Purpose                                                     | Section   |
| -------------------------------- | ----------------------------------------------------------- | --------- |
| `check_localnet_state.ts`        | Report localnet `State` (DOWN/STALE/UNREADY/READY)          | §1        |
| `run_test.ts`                    | Run a single test by name, file, or swap number             | §4        |
| `generate_event_schemas.ts`      | Regenerate the zod event schemas from runtime metadata      | §5        |
| `perform_swap.ts`                | Run one real end-to-end swap                                | see below |
| `query_storage.ts`               | Read any state chain storage value                          | §9        |
| `list_pallet_config_updates.ts`  | List the governance config items each pallet exposes        | §10       |
| `submit_pallet_config_update.ts` | Change a pallet config item via governance                  | §10       |
| `set_safe_mode.ts`               | Set the runtime safe mode (per-pallet flags) via governance | §11       |

### `perform_swap.ts` — a one-off test swap

Exercises the full deposit → swap → egress path without running a vitest test — handy for generating real swap activity.

```bash
# ./commands/perform_swap.ts <source_asset> <dest_asset> [dest_address]
./commands/perform_swap.ts Eth Usdc            # dest address auto-generated
./commands/perform_swap.ts Btc Eth 0xYourAddr  # explicit dest address
```

Omitting the destination address generates a fresh one for the destination asset. It opens a deposit channel, sends the deposit, and waits through to egress (a couple of minutes).

## 9. Querying a state chain storage value

`./commands/query_storage.ts` is a generic, read-only dedot reader for any pallet storage entry (plain value, map, or n-map). Use it to answer "what is the value of X on the state chain?" and to verify a config change. Pallet and entry names are the camelCase dedot keys.

```bash
cd bouncer
./commands/query_storage.ts                                  # list pallets that have storage
./commands/query_storage.ts swapping                         # list swapping's storage entries
./commands/query_storage.ts --search loan                    # find entries across ALL pallets (name + docs)
./commands/query_storage.ts swapping networkFeeForAsset Btc  # map + full key -> exact value (e.g. 15000)
./commands/query_storage.ts swapping networkFeeForAsset      # map + no key  -> dump ALL entries
./commands/query_storage.ts swapping collectedNetworkFee     # plain value
./commands/query_storage.ts <pallet> <entry> <partialKey>    # n-map + partial key -> prefix dump

# Any network — read-only, so safe against mainnet. Flags may appear anywhere in the args.
./commands/query_storage.ts --network mainnet swapping networkFee        # known nets: mainnet/berghain/perseverance/sisyphos/localnet
./commands/query_storage.ts --endpoint wss://my.node swapping networkFee  # any custom ws endpoint
CF_NODE_ENDPOINT=wss://mainnet-rpc.chainflip.io ./commands/query_storage.ts swapping networkFee  # env-var form
```

- **Finding an item without knowing its pallet:** `--search <term>` (alias `--find`) substring-matches across every `pallet.entry` name and its docs, returning `{ pallet, entry, docs }` pairs to query directly. It matches storage entry _names_, not nested struct fields — e.g. `--search lending` surfaces `lendingConfig`, whose decoded value then holds `minimumLoanAmountUsd`.
- **Discovery is built in:** no args lists pallets; `<pallet>` alone lists that pallet's entries; an unknown pallet/entry errors with the valid options.
- **How many keys to pass is auto-detected** from the entry's metadata — no flag. Pass the full key set for an exact lookup; pass fewer keys (or none) to dump all matching entries (a no-key map dumps everything; a partial key prefix-filters an n-map). A plain `StorageValue` takes no keys and is read directly — if it holds a map (e.g. `tradingStrategy.minimumDeploymentAmountForStrategy`), the read returns the whole collection.
- **Keys** are parsed as JSON when valid (`5` → number, `{"chain":"Bitcoin"}` → object), otherwise treated as a string (`Btc`). Pass large integers as quoted decimal strings (decoded to BigInt).
- **Output** is JSON; an unset entry (no stored value/default) prints `null`. Decoded enums render as `{ "type", "value" }`, accounts as SS58 strings.
- **Target network** defaults to localnet. Override with `--endpoint <wss-url>` (highest precedence), `--network <name>`, or `CF_NODE_ENDPOINT`. It's metadata-driven, so it adapts to whatever runtime the remote chain serves — no regenerated chaintypes needed. The chosen endpoint is echoed to stderr; stdout stays pure JSON.

## 10. Updating a pallet config item

Many pallets expose a governance-gated `update_pallet_config` extrinsic — the canonical way to change a runtime config knob (network fees, durations, limits, safe-mode, etc.). Two commands cover discover → submit; verify with §9. This **changes on-chain state via a snowWhite governance proposal** (it auto-executes on localnet), so always `--dry-run` first.

**1. Discover** the config items a pallet exposes:

```bash
cd bouncer
./commands/list_pallet_config_updates.ts            # all pallets, JSON
./commands/list_pallet_config_updates.ts swapping   # filter by pallet (substring)
```

Each entry lists its variants in dedot's `{ type, value }` form plus an `arity` (`array` or `single`). Example variant: `{ "type": "SetNetworkFeeForAsset", "value": { "asset": "enum Asset: Eth|Flip|...", "rate": "Option<u32>" } }`.

**2. Craft + dry-run.** Build a JSON **array** of `{ type, value }` variants (always an array — the submit command adapts to single-arity pallets, and errors if you pass more than one element to one). Dry-run encodes + validates the call without submitting:

```bash
echo '[{"type":"SetNetworkFeeForAsset","value":{"asset":"Btc","rate":15000}}]' \
  | ./commands/submit_pallet_config_update.ts swapping - --dry-run
```

The `updates` argument can be a literal JSON string, `@path/to/file.json`, or `-` (stdin).

**3. Submit** for real by dropping `--dry-run`. Prints the governance proposal id:

```bash
echo '[{"type":"SetNetworkFeeForAsset","value":{"asset":"Btc","rate":15000}}]' \
  | ./commands/submit_pallet_config_update.ts swapping -
```

**4. Verify** with §9: `./commands/query_storage.ts swapping networkFeeForAsset Btc` → `15000`.

Gotchas:

- **Units are domain-specific and not shown by the listing.** A field typed `u32`/`u128` may really be a `Permill` (parts-per-million), a USD/USDC amount (6 decimals), a block count, etc. Convert before submitting — e.g. 150 bps = 1.5% = **15000** Permill; 1000 USDC = **1000000000** atomic units; $200 = **200000000** (6-dp USD). Check the pallet's `PalletConfigUpdate` enum in `state-chain/pallets/<pallet>/src/lib.rs` when unsure.
- **Use the field keys exactly as listed — they're camelCase** (`minimumLoanAmountUsd`), matching dedot's codec. Passing the Rust snake_case form is rejected with an opaque `ApiCompatibilityError: invalid input type`.
- **Integers wider than 32 bits must be quoted strings** (`u64`/`u128`/…, which the listing annotates `(pass as string)`) — decoded to BigInt; a plain JSON number is rejected even when it fits in a JS number. `u32` and smaller can be plain numbers.
- **Variants that set a group of values set _all_ of them at once** (e.g. lending's `SetMinimumAmounts` takes all four minimums, not just one). Query the current values first (§9) and pass the unchanged ones through, or you'll clobber them.
- **`Option` fields**: include the field for `Some`, omit it for `None` (omitting often clears/removes the entry).
- **Submission needs the localnet indexer running** (the proposal id is read from the `Governance.Proposed` event via the indexer) — fine on any normally-booted localnet.

## 11. Setting the runtime safe mode

`./commands/set_safe_mode.ts` sets the runtime safe mode via governance (`environment.update_safe_mode`). Safe mode is a single struct of per-pallet flags; `CodeAmber` replaces the **whole** struct, so setting one item is a read-modify-write (read current → change one flag → submit the whole struct, other flags preserved). Verify with §9 (`query_storage.ts environment runtimeSafeMode`).

```bash
cd bouncer
./commands/set_safe_mode.ts                                # list the current safe mode (all pallets/flags)
./commands/set_safe_mode.ts swapping swapsEnabled false    # set a boolean flag
./commands/set_safe_mode.ts lendingPools borrowing Red     # set an enum flag  -> { type: 'Red' }
./commands/set_safe_mode.ts witnesser CodeRed              # set a pallet-level enum
./commands/set_safe_mode.ts code-red                       # whole runtime off (CodeRed)
./commands/set_safe_mode.ts code-green                     # whole runtime on  (CodeGreen)
# add --dry-run to any set to encode + print the call without submitting
```

- **Flags aren't all booleans.** The value is coerced to the flag's current type: booleans take `true`/`false`; nested enum flags (e.g. `lendingPools.borrowing`) and pallet-level enums (e.g. `witnesser`) take the variant name (`Red`/`Green`, `CodeRed`/`CodeGreen`). The no-arg listing shows each pallet's flags and current values; unknown pallet/flag errors list the valid options.
- **Same governance caveats as §10**: it submits a snowWhite proposal that auto-executes on localnet, but the submit returns at the _proposed_ stage — execution lands a block or two later, so poll after submitting. Needs the localnet indexer running (proposal id read via the indexer).

## When _not_ to use the bouncer

- Pallet-level changes with no cross-component effect → `cargo nextest run -p <pallet>`.
- Multi-pallet runtime interactions that don't need external chains → `cf-integration-tests` (`cargo nextest run -p cf-integration-tests`).
- Anything that doesn't touch the engine, an external chain, or the LP/broker API server → bouncer is overkill and slow.

Reach for the bouncer when the change touches end-to-end flows: the engine, witnessing, threshold signing, broadcasts, the LP/broker JSON-RPC servers, or anything that depends on real BTC/ETH/SOL deposits and broadcasts.
