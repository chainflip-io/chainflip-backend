# Chainflip Development Guidelines

## What is Chainflip?

Chainflip is a decentralised cross-chain swap protocol. Users can swap native assets between different blockchains (e.g. BTC to ETH) without wrapping, bridging, or trusting a centralised intermediary. The protocol is operated by a permissionless set of **Validators** who collectively manage vault wallets on each supported chain via threshold signature schemes (TSS).

### Supported Chains and Assets

Ethereum (ETH, FLIP, USDC, USDT, WBTC), Bitcoin (BTC), Arbitrum (ETH, USDC, USDT), Solana (SOL, USDC, USDT).

### How a Swap Works (High-Level)

1. **Deposit**: A user sends funds to a Chainflip deposit address (or vault) on the source chain.
2. **Witnessing**: Validator engines observe the deposit on the source chain and report it to the State Chain.
3. **Execution**: The State Chain executes the swap through its internal AMM. USDC is used as the intermediate/hub asset for most pairs.
4. **Egress**: The State Chain schedules an output transaction on the destination chain. Validators collaboratively produce a threshold signature, and the transaction is broadcast.

### Core Components

- **State Chain** (`state-chain/`): A Substrate-based (Polkadot SDK) blockchain that is the coordination layer. All protocol logic lives here as FRAME pallets — swap execution, liquidity pools, vault rotation, ingress/egress scheduling, validator auctions, governance, and more. This is the core of the codebase.
- **Chainflip Engine** (`engine/`): An off-chain process run by every Validator alongside their State Chain node. It watches external blockchains (witnessing deposits, tracking gas prices), participates in threshold signing ceremonies (via the multisig sub-crate), and broadcasts signed transactions. Communicates with the State Chain via its RPC/extrinsic interface.
- **Smart Contracts** (external repos): Vault contracts deployed on Ethereum, Arbitrum, and Solana that custody user deposits and execute egress payouts under the authority of the Validator set's aggregate key.
- **FLIP Token**: The native staking and governance token (ERC-20 on Ethereum, also represented on the State Chain). Validators must bond FLIP to participate in the active set. The `cf-flip` pallet manages the on-chain representation; `cf-funding` handles staking.

### Key Concepts

- **Validator Set / Auctions**: Validators compete by staking FLIP. The top N stakers form the active **Authority Set** that manages vaults and processes swaps. Authority sets rotate on a regular cadence (`cf-validator` pallet). Delegators can contribute stake to groups of Validators managed by so-called Operators.
- **Vault Rotation**: When the authority set changes, the aggregate key for each chain's vault is rotated via a key generation ceremony, and the new key is activated and the old one deactivated. In some cases (BTC) this requires migrating fund to a new vault address (`cf-vaults` pallet).
- **Threshold Signing (TSS)**: Validators use multi-party computation to jointly sign transactions without any single party holding the full private key (`cf-threshold-signature` pallet + `engine/multisig/`).
- **Witnessing / Elections**: Validators observe external chain events and reach consensus on what happened via the elections framework (`cf-elections` pallet). This covers deposits, gas price updates, and other chain-tracking data.
- **Liquidity Pools & AMM**: The `cf-pools` pallet implements a novel concentrated-liquidity AMM with limit orders layered on top. Liquidity providers (LPs) deposit assets and set price ranges. The `cf-swapping` pallet routes swaps through the pool(s) and handles features like DCA (chunked execution) and cross-chain messaging (CCM).
- **Ingress / Egress**: `cf-ingress-egress` manages deposit channels (addresses where users send funds) and egress scheduling (batching and sending output transactions).
- **Broadcast**: `cf-broadcast` manages the lifecycle of outgoing transactions on external chains — from threshold signing through to confirmation.

## Build/Test/Lint Commands

- Build: `cargo build --release` or `cargo build -p <package>`
- Lint: `cargo check` or `cargo cf-clippy`
- Lint package: `cargo check -p <package>`
- Format: `cargo fmt -- <filename>` or `cargo fmt --all`
- Run all tests: `cargo nextest run`
- Run package tests: `cargo nextest run -p <package>`
- Run single test: `cargo nextest run <test_name>` or `cargo nextest run <module>::<test_name>`
- Show test output: Add `-- --nocapture` to test commands
- Clean build: `cargo clean` or `cargo clean -p <package>`

## Code Style Guidelines

- Follow Substrate code style (github.com/paritytech/substrate/blob/master/docs/STYLE_GUIDE.md)
- Formatting: 100 char line width, hard tabs, vertical trailing commas
- Errors: Use `Err(anyhow!("message"))` at end of functions, `bail!()` for early returns
- PRs: Keep small (<400 lines), organize meaningful commits
- Prioritize readability and maintainability over cleverness
- Commits: Use prefixes `feat:`, `fix:`, `refactor:`, `test:`, `doc:`, `chore:`
- Run localnet with `./localnet/manage.sh` for testing

## Security

- Never expose, log, or commit secrets or keys
- Security is paramount - follow best practices

## Runtime Safety

Runtime panics must be avoided at all costs. A panic in the runtime hooks halts the chain.

- Never use `.unwrap()`, `.expect()`, array indexing (`[]`), or division without checks in runtime code. The only narrow exception is when the immediate call context *proves* the operation is safe (e.g. you just checked `is_some()` on the same line).
- Use `log_or_panic!` (from `cf-runtime-utilities`) for assertions that should panic in tests but only log an error in production. This is heavily used across pallets.
- Use `#[transactional]` on extrinsics and pallet hooks that modify multiple storage items, so that storage changes are rolled back on error.
- Defensive coding: prefer `.saturating_add()`, `.saturating_sub()`, `.checked_div()`, `ensure!()`, and `ok_or()` patterns in all runtime paths.

## Testing Strategy

### Unit Tests (pallet-level)

Each pallet has its own mock runtime in `src/mock.rs` and tests in `src/tests.rs` (often split into submodules like `tests/fees.rs`, `tests/dca.rs`, etc.).

- Use `impl_mock_chainflip!` and `impl_mock_runtime_safe_mode!` macros (from `cf-traits`) to set up mock runtimes.
- Use `construct_runtime!` with only the pallets needed for the test.
- Use `impl_test_helpers!` (from `cf-test-utilities`) to get a `new_test_ext()` that provides a `TestRunner` with a rich chainable API (`then_execute_with`, `then_execute_at_next_block`, `then_process_blocks`, `then_apply_extrinsics`, etc.).
- Use event assertion macros from `cf-test-utilities`: `assert_has_matching_event!`, `assert_event_sequence!`, `assert_events_match!`, `assert_events_eq!`, `assert_no_matching_event!`.
- For mock traits/APIs, check `state-chain/traits/src/mocks/` first. Reuse existing mocks (e.g. `MockEgressHandler`, `MockPoolPriceApi`, `MockBalance`) rather than creating new ones.
- Design pallets with testability in mind: for external dependencies, prefer traits with clear semantics that can be mocked over concrete implementations.

### Runtime Integration Tests

Full-runtime tests that exercise multiple pallets together. The main crate is `state-chain/cf-integration-tests/` which imports the real `state_chain_runtime` and uses `new_test_ext()` from `test_runner.rs`. Test files cover: `swapping.rs`, `broadcasting.rs`, `threshold_signing.rs`, `witnessing.rs`, `lending.rs`, etc. A `network.rs` module provides network simulation helpers.

Use runtime integration tests when:

- Testing cross-pallet interactions (e.g. swapping triggers egress which triggers broadcast)
- Testing runtime hooks and their ordering
- Verifying migration correctness with the full runtime state (for example if there are cross-pallet dependencies on the migrated data)

### Bouncer Tests (end-to-end)

TypeScript tests in `bouncer/` that run against a localnet. These test the full system including the engine, state chain, and external chains.

Use bouncer tests when:

- Testing end-to-end flows that involve external chains (deposits, broadcasts, witnessing)
- Testing features that depend on the engine (threshold signing, chain tracking)
- Testing time-dependent behaviour across multiple blocks with real chain interaction

### Property-Based Tests (proptests)

Used primarily in `cf-elections` and `cf-trading-strategy` for testing state machines and numerical algorithms.

Proptests are the preferred testing method for any subsystem with clearly defined behaviour and/or invariants.

Use proptests when:

- Testing state machine transitions or consensus algorithms
- Testing numerical/financial calculations where edge cases matter
- Testing properties that should hold for arbitrary inputs (e.g. "price never goes negative")

Proptest regressions are committed to `proptest-regressions/` directories.

## Migrations

### Structure

Each pallet has a `migrations.rs` that defines a `PalletMigration<T>` type alias as a tuple of `VersionedMigration`s, ending with a `PlaceholderMigration`:

```rust
pub type PalletMigration<T> = (
    VersionedMigration<N, N+1, my_migration::Migration<T>, Pallet<T>, <T as frame_system::Config>::DbWeight>,
    PlaceholderMigration<CURRENT_VERSION, Pallet<T>>,
);
```

Individual migrations live in `migrations/my_migration.rs` and implement `UncheckedOnRuntimeUpgrade`.

### Checklist

When writing a migration:

1. **Bump `PALLET_VERSION`** in the pallet's `lib.rs` (the `StorageVersion::new(N)` constant).
2. **Add the migration module** under `migrations/` implementing `UncheckedOnRuntimeUpgrade`.
3. **Update `PalletMigration`** in `migrations.rs`: add a new `VersionedMigration` entry and update the `PlaceholderMigration` version.
4. **Define old storage types** using `#[frame_support::storage_alias]` in an `old` module within the migration file. This avoids depending on types that may change.
5. **Implement `pre_upgrade` and `post_upgrade`** (gated behind `#[cfg(feature = "try-runtime")]`) to verify migration correctness.
6. **For instanced pallets** (e.g. `cf-broadcast`, `cf-ingress-egress`), ensure all instances are migrated. Use `NoopRuntimeUpgrade` for instances that don't need data changes.
7. **Runtime-level migrations** (cross-pallet, one-off) go in `state-chain/runtime/src/migrations/` and implement `OnRuntimeUpgrade` directly. These need to be explicitly added to the runtime migrations: they not automatically included in the same way as pallet migrations.
8. **Test with try-runtime** before merging.

### Placeholder Migration

Always keep a `PlaceholderMigration<VERSION, Pallet<T>>` pointing at the current version to keep the boilerplate consistent and to surface inconsistencies in the pallet storage versions.

## Key Crates and Utilities

### `cf-runtime-utilities` (`state-chain/runtime-utilities/`)

- `PlaceholderMigration` and `NoopRuntimeUpgrade` for migration scaffolding
- `log_or_panic!` macro: panics in tests, logs error in production
- `EnumVariant` derive and `storage_decode_variant` for efficiently decoding enum discriminants from storage
- Genesis hash constants for different networks (Berghain, Perseverance, Sisyphos)
- Migration template at `src/migration_template.rs`

### `cf-utilities` (`utilities/`)

- `derive_common_traits!` / `derive_common_traits_no_bounds!`: derive Debug, Clone, PartialEq, Eq, Encode, Decode, Serialize, Deserialize in one macro
- `define_empty_struct!`: creates PhantomData-based structs with standard derives
- `assert_ok!`, `assert_err!`, `assert_matches!`, `assert_panics!` test helpers
- `impls!` / `hook_impls!`: syntax sugar for implementing multiple traits/election hooks for one type
- `task_scope`, `cached_stream`, `spmc` and other async utilities (std-only)
- `testing::logging` for test log capture

### `cf-test-utilities` (`state-chain/test-utilities/`)

- `TestExternalities` (rich test externalities): chainable API for pallet tests with block processing, context passing, and extrinsic application
- `impl_test_helpers!` macro: sets up `TestRunner` and `new_test_ext()` for a runtime
- Event assertion macros: `assert_has_matching_event!`, `assert_event_sequence!`, `assert_events_match!`, `assert_events_eq!`

### `cf-traits` (`state-chain/traits/`)

- Contains all cross-pallet trait definitions
- `src/mocks/` has reusable mock implementations for testing (MockEgressHandler, MockBalance, MockPoolPriceApi, etc.)
- `impl_mock_chainflip!` macro for setting up mock Chainflip runtimes

### `cf-primitives` (`state-chain/primitives/`)

- Core types: `Asset`, `AssetAmount`, `SwapId`, `ForeignChain`, `ChainflipNetwork`, etc.

### `cf-chains` (`state-chain/chains/`)

The chain abstraction layer. Defines the core `Chain` and `ChainCrypto` traits that all supported blockchains implement, plus per-chain modules (`eth`, `btc`, `dot`, `arb`, `sol`, `hub`, `evm`).

- **`Chain` trait**: defines associated types for each chain: `ChainBlockNumber`, `ChainAmount`, `ChainAsset`, `ChainAccount`, `Transaction`, `TrackedData`, `DepositChannelState`, etc. Every chain type (e.g. `Ethereum`, `Bitcoin`) implements this.
- **`ChainCrypto` trait**: cryptographic types per chain - `AggKey`, `Payload`, `ThresholdSignature`, `TransactionInId/OutId`. Shared across chains with the same crypto (e.g. `EvmCrypto` for Ethereum+Arbitrum, `PolkadotCrypto` for Polkadot+Assethub).
- **API call traits**: `ApiCall`, `AllBatch`, `ExecutexSwapAndCall`, `TransactionBuilder` - builders for constructing on-chain transactions.
- **Address types**: `ForeignChainAddress` (internal enum), `EncodedAddress` (wire format), `AddressConverter` trait for conversion. Flow: `AddressString` (RPC) -> `EncodedAddress` -> `ForeignChainAddress`.
- **CCM types**: `CcmMessage` (max 15KB), `CcmAdditionalData` (max 3KB), `CcmChannelMetadata`, `CcmDepositMetadata`. Checked/unchecked variants for validation pipeline.
- **Pallet instances**: `instances.rs` maps chains to pallet instances (`Ethereum` -> `Instance1`, `Polkadot` -> `Instance2`, `Bitcoin` -> `Instance3`, `Arbitrum` -> `Instance4`, `Solana` -> `Instance5`, `Assethub` -> `Instance6`). Type aliases like `EthereumInstance`, `BitcoinInstance` etc. are used throughout.
- **Fee estimation**: `FeeEstimationApi<C>` implemented on each chain's `TrackedData`, `FeeRefundCalculator<C>` on transactions.
- **`BenchmarkValue`** trait: generates valid test/benchmark values for chain types.

## Benchmarking

Each pallet with extrinsics has a `benchmarking.rs` (gated behind `#[cfg(feature = "runtime-benchmarks")]`) and an auto-generated `weights.rs`.

- Benchmarks use FRAME v2 syntax: `#[benchmarks] mod benchmarks { #[benchmark] fn my_extrinsic() { ... } }`
- Use `BenchmarkValue::benchmark_value()` (from `cf-chains`) to generate valid chain-specific test data for benchmark setup.
- When choosing parameters or initializing state for the benchmarks, aim to measure worst-case performance.
- Weights are generated by running `./chainflip-node benchmark pallet` with `--template=state-chain/chainflip-weight-template.hbs` and output to the pallet's `weights.rs`.
- Each pallet defines a `WeightInfo` trait in `weights.rs` with one method per benchmarked extrinsic, and a `PalletWeight<T>` struct implementing it.
- Extrinsics reference weights via `#[pallet::weight(T::WeightInfo::my_extrinsic())]`.
- Weights files are auto-generated - do not edit them by hand. If you add or change an extrinsic, add a corresponding benchmark and regenerate.

## Smart Contracts

The on-chain smart contracts for external chains live in separate repositories:

- **Ethereum/Arbitrum**: <https://github.com/chainflip-io/chainflip-eth-contracts>
- **Solana**: <https://github.com/chainflip-io/chainflip-sol-contracts>

These define the vault contracts, token vaults, and swap endpoints that the state chain and engine interact with. Changes to contract ABIs or behavior may require corresponding updates in `cf-chains`, the engine, and/or bouncer tests.

## Bouncer (TypeScript)

The `bouncer/` directory contains end-to-end tests and operational scripts.

### Key Patterns

- **Use `ChainflipIO`** (`bouncer/shared/utils/chainflip_io.ts`) for all state chain interactions. It tracks block heights for event ordering and provides type-safe extrinsic submission and event waiting. Prefer extending `ChainflipIO` over writing ad-hoc queries.
- **Generated event types** live in `bouncer/generated/events/` with zod schemas for type-safe event parsing.
- **Use the indexer** (`bouncer/shared/utils/indexer.ts`) for querying events by block range, not direct RPC polling.
- Tests use `vitest` with `concurrentTest` / `serialTest` helpers for parallel/serial execution.
- Test files go in `bouncer/tests/`, shared utilities in `bouncer/shared/`, CLI commands in `bouncer/commands/`.

## Project Structure (Key Directories)

```text
state-chain/
  pallets/           # Substrate pallets (cf-swapping, cf-ingress-egress, cf-pools, etc.)
  runtime/           # Runtime configuration, migrations, APIs
  traits/            # Cross-pallet traits and mock implementations
  primitives/        # Core types shared across pallets
  chains/            # Chain-specific types and logic
  runtime-utilities/ # Migration helpers, log_or_panic, etc.
  test-utilities/    # Rich test externalities, event assertion macros
engine/              # Off-chain engine (witnessing, signing, broadcasting)
utilities/           # General Rust utilities (macros, async helpers, etc.)
bouncer/             # TypeScript end-to-end tests and operational scripts
foreign-chains/      # Foreign chain integration code (Solana primitives, etc.)
localnet/            # Local development network scripts
```
