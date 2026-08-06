# Chainflip Development Guidelines

## What is Chainflip?

Chainflip is a decentralised cross-chain swap protocol. Users can swap native assets between different blockchains (e.g. BTC to ETH) without wrapping, bridging, or trusting a centralised intermediary. The protocol is operated by a permissionless set of **Validators** who collectively manage vault wallets on each supported chain via threshold signature schemes (TSS).

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

- Lint: `cargo check` or `cargo cf-clippy`
- Format: `cargo fmt --all` (never `cargo fmt -- <filename>`: the per-file form ignores the crate's edition and applies the wrong import-ordering style)
- Run all tests: `cargo nextest run`
- Run package tests: `cargo nextest run -p <package>`
- Run single test: `cargo nextest run <test_name>` or `cargo nextest run <module>::<test_name>`

## Code Style Guidelines

- Formatting: All formatting rules are imposed by `cargo fmt --all`, run this before every commit.
- Errors: Use `Err(anyhow!("message"))` at end of functions, `bail!()` for early returns
- PRs: Keep small (<400 lines), organize meaningful commits
- Prioritize readability and maintainability over cleverness
- Commits: Use prefixes `feat:`, `fix:`, `refactor:`, `test:`, `doc:`, `chore:`
- Run localnet with `./localnet/manage.sh` for testing

### Comments and Doc Comments

Keep comments and doc comments concise; use them to add context, rather than to explain what the code does (the code documents the implementation).

- Explain *why* — rationale, non-obvious invariants, edge cases, links to specs/issues. Don't narrate *what* the code does or walk through control flow.
- Don't reference external implementation details or superseded designs/decisions (e.g. "previously we…", "this used to…").

## Security

- Never expose, log, or commit secrets or keys
- Security is paramount - follow best practices

## Runtime Safety

Runtime panics must be avoided at all costs. A panic in the runtime hooks halts the chain.

- Never use `.unwrap()`, `.expect()`, array indexing (`[]`), or division without checks in runtime code. The only narrow exception is when the immediate call context *proves* the operation is safe (e.g. you just checked `is_some()` on the same line).
- Use `log_or_panic!` (from `cf-runtime-utilities`) for assertions that should panic in tests but only log an error in production. This is heavily used across pallets.
- Use `#[transactional]` on extrinsics and pallet hooks that modify multiple storage items, so that storage changes are rolled back on error.
- Defensive coding: prefer `.saturating_add()`, `.saturating_sub()`, `.checked_div()`, `ensure!()`, and `ok_or()` patterns in all runtime paths.

## Account Lifecycle

When a pallet adds per-account storage (maps keyed by `AccountId`, or per-account entries in a shared structure), it must also clean that storage up when the account is deregistered.
Wire the cleanup into the runtime's `frame_system` `OnKilledAccount` handler.

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

For running bouncer tests, managing the localnet, regenerating event schemas, or any related lifecycle task, use the `bouncer` skill in `.claude/skills/bouncer/`.

### Property-Based Tests (proptests)

Used primarily in `cf-elections` and `cf-trading-strategy` for testing state machines and numerical algorithms.

Proptests are the preferred testing method for any subsystem with clearly defined behaviour and/or invariants.

Use proptests when:

- Testing state machine transitions or consensus algorithms
- Testing numerical/financial calculations where edge cases matter
- Testing properties that should hold for arbitrary inputs (e.g. "price never goes negative")

Proptest regressions are committed to `proptest-regressions/` directories.

## Migrations

For writing storage migrations (structure, checklist, try-runtime verification), use the `writing-migrations` skill.

## Runtime API Versioning

Parallel concern to [Migrations](#migrations): pallet storage versioning covers on-chain state shape; runtime-API versioning covers the wire shape so the *current* node/RPC code can query historical blocks (or sync from genesis) whose runtime was on an older version.

The rule fires on any change that alters the SCALE-encoded wire shape of a `CustomRuntimeApi` method, including:

- Adding, renaming, or removing a method on the trait.
- Changing a parameter or return type of an existing method.
- **Changing the fields of a type that's reachable from a method signature**, even if that type lives in a pallet (e.g. `pallet_cf_lending_pools::RpcLendingPool`). Adding/removing/reordering/retyping a field counts. The compiler won't flag this as an API change — the type is just a struct — but the wire encoding still moves.

When in doubt, ask: "would a SCALE-encoded value of this type round-trip through an older node?". If no, version it.

Follow the procedure in the header comment of `state-chain/runtime/src/runtime_apis/custom_api.rs` — that comment is the source of truth. Read it before editing the trait, the `custom_api/types/before_version_*.rs` modules, the runtime impls, or `state-chain/custom-rpc/src/lib.rs` (where the version dispatch lives).

Pure docs/internal refactors that don't change the wire shape don't need this.

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

For adding pallet benchmarks or regenerating weights, use the `writing-benchmarks` skill.

## Smart Contracts

The on-chain smart contracts for external chains live in separate repositories:

- **Ethereum/Arbitrum**: <https://github.com/chainflip-io/chainflip-eth-contracts>
- **Solana**: <https://github.com/chainflip-io/chainflip-sol-contracts>

These define the vault contracts, token vaults, and swap endpoints that the state chain and engine interact with. Changes to contract ABIs or behavior may require corresponding updates in `cf-chains`, the engine, and/or bouncer tests.

Bouncer code patterns live in `bouncer/CLAUDE.md` (loaded when working under `bouncer/`).
