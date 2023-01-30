# Substrate Troubleshooting

This is a living document with tips, gotchas and general subtrate-related wizardry. If you ever get stuck with an incomprehensible compiler error, before spending the day turning in circles, come here first and see if someone else has already encountered the same issue.

Please add anything you think will save your colleagues' precious time.

If you come across anything that is inaccurate or incomplete, please edit the document accordingly.

As of yet there is no real structure - this isn't intended to be a document to read from start to finish, it's not a tutorial. But your entries should be searchable, so write your entries with SEO in mind.

## Runtime upgrades / Try-runtime

First, build the runtime node with all features enabled:

> *Note: you need to tweak the `spec_version` of the local runtime to match that of the remote chain.*

```sh
cargo build --release --all-features
```

It's theoretically possible to connect to a remote rpc node to download the state for the try-runtime checks, but a much faster method is to run a local rpc node and connect to that instead.

For example, for perseverance, first connect a local node to the network with some rpc optimisations:

```sh
# Purge any pre-existing chain state.
./target/release/chainflip-node purge-chain --chain ./state-chain/node/chainspecs/perseverance.chainspec.raw.json

# Sync a fresh copy of the latest perseverance state.
./target/release/chainflip-node \
    --chain ./state-chain/node/chainspecs/perseverance.chainspec.raw.json 
    --sync warp \
    --rpc-max-request-size 100000 \
    --rpc-max-response-size 100000 \
    --rpc-external \
    --rpc-cors all \
    --unsafe-ws-external
```

Once the node has synced, in another terminal window, run the checks:

```sh
./target/release/chainflip-node try-runtime --execution native \
    on-runtime-upgrade live --uri wss://perseverance-rpc.chainflip.io:9944
```

> *Note: Using `--execution native` ensures faster execution and also prevents log messages from being scrubbed.

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
cargo cf-build-with-benchmarks
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
cargo cf-test-all
```

> **NOTE:**  When you run your benchmark with the tests it's **NOT** running against the runtime but against the mocks. If the behaviour of the mocks doesn't match the behaviour of the runtime, it's possible that tests will fail despite benchmarks succeeding, or that benchmarks will fail despite the tests succeeding.

### Some benchmark reference values

Benchmark weight is measured in picoseconds. Our block budget is 6 seconds, or 6_000_000_000_000 weight units.

The typical order of magnitude for extrinsic and data access weights is in the millions of weight units, which is equivalent to microseconds.

Typical values for runtime data access speeds for rocksdb are 25µs for a read and 100µs for a write.

Typical values for extrinsic *execution*, ie. not including reads and writes, are around 30µs to 60µs.

In other words, reads and writes are *expensive* and writes in particular should be kept to a minimum. A single read is as expensive as a moderately complex extrinsic. We should avoid iterating over storage maps unless the size is tightly bounded.

## Runtime Panics

We should always convince ourselves that our runtime code *can't* panic at runtime. This means thorough testing of the full range of possible inputs to any function that *might* panic according to the compiler.

Anywhere we know we can't panic, but the compiler can't guarantee it, it's acceptable to follow parity's conventions of using `expect("reason why this can't panic")`.

There are grey areas. For example, it's acceptable to panic on any condition that indicates that a block has been faultily or maliciously authored. As a concrete example, if the digest is invalid, this indicates a problem with the node, not the runtime, so it's acceptable for the runtime to panic in response - presumably the author has been tampering with their node software.

### Benchmark whitelist

When writing benchmarks, storage keys can be •whitelisted• for reads and/or writes, meaning reading/writing to the
whitelisted key is ignored. This is confusing since mostly this is used in the context of whitelisting *account*
storage, which is easy to confuse with whitelisting the actual account.

See [this PR](https://github.com/paritytech/substrate/pull/6815) for a decent explanation.

## Cargo.toml's std section

The substrate runtime (and by extension, all the pallets and their dependencies) need to be able to compile without the standard library. So each of their Cargo.toml files pulls in the dependencies with `default-features = false` to exclude anything that requires `std` to be enabled.

However for testing and also for native execution, the runtime *can* make use of `std` features. So we can instruct the compiler to *include* these features only if the `std` feature is activated. You can also use this to activate optional features that are only available for certain compilation targets, for example.

For example, if you have something like this:

``` toml
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

## Substrate storage: Separation of front overlay and backend. Feat clear_prefix()

### Overview

Substrate storage has 4 layers that work together to facilitate storage read/write in a database manner. They are, from top to bottom

**Runtime storage API**: StorageValue, StorageMap, StorageDoubleMap etc.

**Overlay Change Set**: Changes to storage is stored in this overlay change set for the duration of the block, and is only committed to the backend database storage once per block

**Merkle Trie**: A tree data structure for data hashes, helps with data retrieval

**Key Value Database**: Bottom layer DB to store data

### Use of clear_prefix() for cleaning double map storages

``` rust
/// Remove all values under the first key `k1` in the overlay and up to `maybe_limit` in the backend.
/// All values in the client overlay will be deleted, if `maybe_limit` is `Some` then up to that number of values are deleted from the client backend, otherwise all values in the client backend are deleted.
fn clear_prefix<KArg1>(
 k1: KArg1,
 limit: u32,
 maybe_cursor: Option<&[u8]>,
) -> sp_io::MultiRemovalResults
```

**prefix**: the first Key value in which you want all the data to be cleaned up

**limit**: max number of data to be deleted from the *Backend*

**maybe_cursor**: Optional cursor to be passed in. This needs to be passed in if multiple `clear_prefix` is called within the same block. Otherwise all calls will delete the same items (even after they are already deleted), due to async nature of Overlay and DB backend.

``` rust
pub struct MultiRemovalResults {
 /// A continuation cursor which, if `Some` must be provided to the subsequent removal call.
 /// If `None` then all removals are complete and no further calls are needed.
 pub maybe_cursor: Option<Vec<u8>>,
 /// The number of items removed from the backend database.
 pub backend: u32,
 /// The number of unique keys removed, taking into account both the backend and the overlay.
 pub unique: u32,
 /// The number of iterations (each requiring a storage seek/read) which were done.
 pub loops: u32,
}
```

Note: At the time of writing, backend, unique and loops all return the same thing.

### Example Usage

In a usual unit test, the whole test is done within the same block.
Storage modified are stored on the Overlay Change Set. e.g.

``` rust
Data::<T>::insert(1, Alice, 100);
Data::<T>::insert(1, BOB, 33);

let res = Data::clear_prefix(1, 0, None);
// res.maybe_cursor == None, res.unique = 0;
assert!(Data::<T>::get(1, Alice), None);
assert!(Data::<T>::get(1, BOB), None);
```

Even if `limit` is set as 0, both entries are still deleted.
To be able to test deletion in the backend database, the change overlay will have to be committed into the DB. This can be done with `ext.commit_all()`

``` rust
let mut ext = new_test_ext();
ext.execute_with(|| {
 // Data modification to the Overlay Change Set
 Data::<T>::insert(1, Alice, 100);
 Data::<T>::insert(1, BOB, 33);
});

// Commit the changeset to the DB backend
let _ = ext.commit_all();

ext.execute_with(|| {
 // Test deletion of backend DB entries
 let res = Data::clear_prefix(1, 1, None);
 // res.maybe_cursor == ptr(1, BOB), res.unique = 1;
 assert!(Data::<T>::get(1, Alice), None);
 assert!(Data::<T>::get(1, BOB), Some(33));
});
```

### Summary TL;DR

- How clear_prefix works is a demonstrate on how Overlay Change Set interacts with the Backend DB.

- If you need to test backend DB deletion you can use `ext.commit_all();` to manually push your changeset into the backend.

### Reference

clear_prefix docs:
<https://github.com/paritytech/substrate/blob/dc45cc29167ee6358b527f3a0bcc0617dc9e4c99/frame/support/src/storage/mod.rs#L534>

Storage PDF:
<https://www.shawntabrizi.com/assets/presentations/substrate-storage-deep-dive.pdf>

commit_all() doc and impl:
<https://github.com/paritytech/substrate/blob/a5f349ba9620979c3c95b65547ffd106d9660d9d/primitives/state-machine/src/testing.rs#L379>
