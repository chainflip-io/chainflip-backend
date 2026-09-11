# Limit-order swap performance

`swap_single_leg` is an opt-in FRAME benchmark (`--extra`). It measures the actual
pool storage read/decode, AMM execution, LP balance credits, statistics, events,
pool encoding/write and transaction handling. Node runs use the real runtime
dependencies; the pallet's mock-runtime tests only verify fixture correctness.
It does not change runtime weights or enforce a limit. The runner requires Python 3.9+.

Build and run from the repository root on the reference validator hardware:

```sh
cargo build --release --locked -p chainflip-node --features runtime-benchmarks
python3 state-chain/scripts/benchmark-swaps.py \
  --output-dir /tmp/swap-benchmarks-01 \
  --orders 1 12 100 1000 10000 \
  --order-usd 5 10 \
  --budget-ms 1000 --attempts 2 --margin 2
```

The budget, attempt count and margin above are illustrative choices, not protocol
limits. `--binary` selects another build. Use an optimized, uninstrumented runtime
with production-equivalent settings. Record the build profile and run on an idle
machine; the manifest records the binary hash, checkout, host and exact commands.
`--heap-pages` optionally fixes the WASM heap allocation (64 KiB pages).

Start with a small subset to check the build:

```sh
python3 state-chain/scripts/benchmark-swaps.py \
  --output-dir /tmp/swap-benchmarks-smoke \
  --orders 1 12 --pairs btc_sell eth_buy \
  --configurations partial_many_lps price_impact_rollback --repeat 2
```

## Standard hardware runs

`_100_run_benchmarks.yml` automatically runs this suite after pallet weight
generation, sequentially on the same dedicated runner using the same downloaded
binary and profile. By default it covers all configurations and both directions
of both pairs at 1, 12, 100, 1,000 and 10,000 orders, worth $10 each. The workflow's
`swap_order_counts` and `swap_order_usd` inputs override these space-separated lists
(for example `5 10` for order values). Its `repetitions` input controls external
repetitions; `steps` only affects the ordinary pallet benchmarks.

The Actions summary contains a table of measurements, and the generated weight PR
links to the run. Download `swap-benchmarks-<commit>-<profile>-<machine>` for CSV,
raw JSON, logs, the budget screen, Markdown summary and manifest. The manifest
records the downloaded binary's source commit separately from the script checkout
and hashes the binary. CI uses the runner's default illustrative budget of 1,000 ms
with a 2× margin; this does not set a protocol limit.

The swap step has a 120-minute timeout within a 180-minute job. Partial results
are uploaded on failure, the failure is reported, and runner teardown still runs.
Selecting an older binary without this benchmark produces an explicit skip notice;
ordinary weight generation still works.

## Configurations

`n` is the number of main orders. Every case also has one extra order, so the
total is `n + 1`. Unless stated otherwise, the main orders belong to distinct LPs
and have no pre-existing proceeds balance or LP statistics for the input asset.

| ID | Configuration | Main orders and swap |
|---|---|---|
| 0 | `partial_many_lps` | One tick, approximately one-third filled |
| 1 | `partial_one_lp` | Same as 0, but one LP owns all main orders |
| 2 | `full_tick` | All main orders at one tick filled and removed |
| 3 | `full_12_per_tick` | Fill all main orders, grouped 12 per tick |
| 4 | `untouched_ticks` | Main orders at worse ticks; partially fill only the extra order |
| 5 | `untouched_opposite_side` | Main orders on the opposite side; partially fill only the extra order |
| 6 | `one_atom_input` | One tick, swap one atomic input unit; visits all main orders, possibly producing no fills |
| 7 | `price_impact_rollback` | Fill main orders at 12 per tick and part of the extra order, then reject on price impact |
| 8 | `partial_existing_accounts` | Same as 0, with existing proceeds balances and statistics |
| 9 | `full_one_per_tick` | Fill all main orders, one per tick |
| 10 | `fixed_swap_value` | One tick, swap half of one order's value regardless of `n` (about $5 at the default order value) |

The extra order normally remains at a worse price because `swap_single_leg`
requires a price after execution. Cases 4/5 instead use it as the sole active
order. Every timed region contains exactly one call to `swap_single_leg`.
Setup constructs the book once in memory and persists it before timing. This
avoids measuring order placement and avoids quadratic setup from repeated pool
decoding. Verification checks success versus the expected error, sold-liquidity
conservation, number of changed/removed orders, cache counts and final events.

Pairs are `btc_sell`, `btc_buy`, `eth_sell`, `eth_buy`; sell/buy names refer to the
**LP order side**. The fixed starting ticks are 69082 for BTC and -200000 for ETH
(roughly $100,000/BTC and $2,063/ETH). These are synthetic prices, not live quotes.
`--order-usd` sets the quote value of each order at its tick. Base quantities round
up to atomic units. Thus BTC uses 8/6 decimals and ETH uses 18/6. At extreme ticks,
the smallest base unit can materially exceed the requested dollar value.

## Results and interpretation

The runner saves raw FRAME JSON, logs, a manifest, `results.csv`, `summary.md` and
`budget-screen.json`. It stops increasing `n` for a configuration at the first
tested point exceeding the budget. Errors/timeouts abort the run and preserve
completed results; they are not reported as fast or successful measurements.

- `median_ms` / `max_ms`: measured compiled-WASM execution time (FRAME reports ns).
- `reads` / `writes`: distinct storage accesses, with repeated accesses separately
  reported; `proof_bytes` is the measured proof size.
- `max_storage_root_ms`: FRAME's separate storage-root measurement. Reported
  separately because block-level root calculation is shared across work.
- `reference_ms`: maximum observed execution time plus reads/writes charged at
  the runtime's current `ParityDbWeight` (8 µs/read, 50 µs/write). This is a weight
  estimate, not an additional wall-clock measurement.
- `budget_cost_ms`: `reference_ms * attempts * margin`. The budget screen reports
  tested points only and performs no linear extrapolation.

The benchmark components are `(n, c, a, v)`: count, configuration ID, pair ID
(0–3 in the order above), dollar value. The runner pins all four ranges to avoid
regressing categorical IDs as though they were numerical costs. FRAME's minimum
two steps visits a pinned point repeatedly; all samples are retained. External
repetitions use one internal repetition to avoid retaining successive fixtures
in the same WASM invocation. Counts up to 100,000 and dollar values up to 1,000,000
can be supplied for stress runs, beyond the default FRAME ranges.

Use the worst relevant configuration to bracket a candidate count, then rerun
with finer counts near that boundary. An empirical ceiling is not a proven hard
limit: also account for other block work, mixed range liquidity, multiple pools,
batch orchestration and repeated failed attempts. This benchmark includes one
late rollback, not the full batch retry loop, and does not report peak memory.
Validate the eventual limit with full-block stress tests and production WASM
memory settings before relying on it for chain safety.

A minimum order size can inform the capital needed to construct such a book
(approximately count × minimum value). It does not bound the count by itself.
Small swaps still visit the orders at their execution ticks, untouched orders
still contribute to pool decoding, and partial fills can leave orders below the
placement minimum. Do not divide swap value by the minimum to infer work.

Fixture and runner checks:

```sh
cargo nextest run -p pallet-cf-pools --features runtime-benchmarks,try-runtime benchmarking
python3 -B -m unittest discover -s state-chain/scripts/tests -p 'test_benchmark_swaps.py'
```
