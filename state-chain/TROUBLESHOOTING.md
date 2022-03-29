# Substrate Troubleshooting

This is a living document with tips, gotchas and general subtrate-related wizardry. If you ever get stuck with an incomprehensible compiler error, before spending the day turning in circles, come here first and see if someone else has already encountered the same issue.

Please add anything you think will save your colleagues' precious time.

If you come across anything that is inaccurate or incomplete, please edit the document accordingly.

As of yet there is no real structure - this isn't intended to be a document to read from start to finish, it's not a tutorial. But your entries should be searchable, so write your entries with SEO in mind.

## Runtime upgrades / Try-runtime

First, build the runtime node with all features enabled:

```sh
cargo build --release --all-features
```

Then run a variation of the following command.

```sh
./target/release/chainflip-node try-runtime \
    --execution Native \
    --chain soundcheck \
    --url wss://bashful-release.chainflip.xyz \
    --block-at <SET TO A RECENT BLOCK ON CHAIN UPGRADING FROM> on-runtime-upgrade live \
    --snapshot-path .state-snapshot
```

To save time, you can then use the state snapshot in subsequent runs:

```sh
./target/release/chainflip-node try-runtime \
    --execution Native \
    --block-at <SET TO A RECENT BLOCK ON CHAIN UPGRADING FROM> on-runtime-upgrade snap \
    --snapshot-path .state-snapshot
```

### General tips and guidelines

- There are some useful storage conversion utilities in `frame_support::storage::migration`.
- Don't forget the add the `#[pallet::storage_version(..)]` decorator.
- Use the `ensure!` macro in pre- and post-upgrade check to get meaningful error messages.
- Use `--execution Native` to ensure that `Debug` variables are not replaced with `<wasm::stripped>`.

You can write the runtime upgrade as part of the Chainflip runtime rather than using the
pallet hooks. Depending on the situation, one or the other option might be easier or more
appropriate. For example, migrations that span multiple pallets are easier to write as a
runtime-level migration.

### Storage migration / OnRuntimeUpgrade hook doesn't execute

Make sure you build with `--features try-runtime`.
Make sure you have incremented the spec version and/or the transaction version in `runtime/lib.rs`.
Make sure you are testing against a network that is at a lower version number!

### Pre and Post upgrade hooks don't execute

Make sure to add `my-pallet/try-runtime` in the runtime's Cargo.toml, otherwise the feature will not be activated for the pallet when the runtime is compiled.

## Benchmarks

### Compile the node with benchmark features enabled

```bash
cargo build --release --features runtime-benchmarks
```

### Generating weight files

To generate or update a single weight file for pallet run:

```bash
source state-chain/scripts/benchmark.sh {palletname e.x: broadcast}
```

### Chainflip-Node is not compiling with benchmark features enabled

Due to a compiler bug, you have to clean the state-chain after every successful compiler run:

```bash
cargo clean -p chainflip-node
```

### Debug Benchmarks in VSCode

It can be useful to have the ability to debug a benchmark properly (especially if you didn't write the code). To do this add the following configuration to your `launch.json` and replace `{your_pallet}` with the pallet name:

```json
    {
        "type": "lldb",
        "request": "launch",
        "name": "Debug Benchmark",
        "program": "./target/debug/chainflip-node",
        "args": [
            "benchmark",
            "--extrinsic",
            "*",
            "--pallet",
            "pallet_cf_{your_pallet}",
            "--steps",
            "2",
            "--repeat",
            "2"
        ],
        "cwd": "${workspaceFolder}"
    }
```

To start debugging build the node with benchmark features enabled as well as debug symbols (no `--release`).

### Speed up cycle time

Benchmarking in general has a very slow cycle time, you can speed it up by running the benchmarks in a test environment though. Add the following line to the end of your benchmark to execute your benchmark with the test suite and **against the mock** of your pallet:

```rust
impl_benchmark_test_suite!(
    Pallet,
    crate::mock::new_test_ext(Default::default(), Default::default()),
    crate::mock::Test,
);
```

Unfortunately, you have to run all tests in the state-chain, otherwise you will get the following error:

```bash
error[E0432]: unresolved import `sp_core::to_substrate_wasm_fn_return_value`
  --> /Users/janborner/.cargo/git/checkouts/substrate-a7ad12d678bd31ac/e563465/primitives/api/src/lib.rs:80:9
   |
80 | pub use sp_core::to_substrate_wasm_fn_return_value;
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ no `to_substrate_wasm_fn_return_value` in the root
```

To execute the benchmarks as tests run:

```bash
cargo test --lib --all-features
```

> **_NOTE:_**  When you run your benchmark with the tests it's **NOT** running against the runtime but the mocks. If you make different assumptions in your mock it can be possible that the tests will fail.
