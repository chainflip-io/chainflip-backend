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
    --chain paradise \
    --url wss://bashful-release.chainflip.xyz \
    --block-at <SET TO A RECENT BLOCK ON CHAIN UPGRADING FROM> \
        on-runtime-upgrade live \
            --snapshot-path .state-snapshot
```

To save time, you can then use the state snapshot in subsequent runs:

```sh
./target/release/chainflip-node try-runtime \
    --execution Native \
    --block-at <SET TO A RECENT BLOCK ON CHAIN UPGRADING FROM> \
        on-runtime-upgrade snap \
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

### Some benchmark reference values

Benchmark weight is measured in picoseconds. Our block budget is 6 seconds, or 6_000_000_000_000 weight units.

The typical order of magnitude for extrinsic and data access weights is in the millions of weight units, which is equivalent to microseconds.

Typical values for runtime data access speeds for rocksdb are 25µs for a read and 100µs for a write.

Typical values for extrinsic _execution_, ie. not including reads and writes, are around 30µs to 60µs.

In other words, reads and writes are _expensive_ and writes in particular should be kept to a minimum. A single read is as expensive as a moderately complex extrinsic. We should avoid iterating over storage maps unless the size is tightly bounded.

## Runtime Panics

We should always convince ourselves that our runtime code _can't_ panic at runtime. This means thorough testing of the full range of possible inputs to any function that _might_ panic according to the compiler.

Anywhere we know we can't panic, but the compiler can't guarantee it, it's acceptable to follow parity's conventions of using `expect("reason why this can't panic")`.

There are grey areas. For example, it's acceptable to panic on any condition that indicates that a block has been faultily or maliciously authored. As a concrete example, if the digest is invalid, this indicates a problem with the node, not the runtime, so it's acceptable for the runtime to panic in response - presumably the author has been tampering with their node software.

### Benchmark whitelist

When writing benchmarks, storage keys can be •whitelisted• for reads and/or writes, meaning reading/writing to the
whitelisted key is ignored. This is confusing since mostly this is used in the context of whitelisting _account_
storage, which is easy to confuse with whitelisting the actual account.

See [this PR](https://github.com/paritytech/substrate/pull/6815) for a decent explanation.

## Cargo.toml's std section

The substrate runtime (and by extension, all the pallets and their dependencies) need to be able to compile without the standard library. So each of their Cargo.toml files pulls in the dependencies with `default-features = false` to exclude anything that requires `std` to be enabled.

However for testing and also for native execution, the runtime *can* make use of `std` features. So we can instruct the compiler to *include* these features only if the `std` feature is activated. You can also use this to activate optional features that are only available for certain compilation targets, for example.

For example, if you have something like this:

```
[package]
name = 'my-crate'

[dependencies]
my-dep = { version = "1", default-features = false }
my-optional-dep = { version = "1", optional = true }

[features]
std = ['my-dep/std', 'my-optional-dep']
```

It means that, by default, `my-crate` will pull in `my-dep` without any default features activated, and will *not* pull in `my-optional-dep` at all.

However if you compile `my-crate` with feature `std`, then it will pull `my-dep` *and* `my-optional-dep` with `std` feature activated.
