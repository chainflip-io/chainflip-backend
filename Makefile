.PHONY: init
init:
	./scripts/init.sh

.PHONY: check
check:
	SKIP_WASM_BUILD=1 cargo check

.PHONY: check-chainflip-transactions
check-chainflip-transactions:
	SKIP_WASM_BUILD=1 cargo check --manifest-path pallets/chainflip-transactions/Cargo.toml

.PHONY: test
test:
	SKIP_WASM_BUILD=1 cargo test --all

.PHONY: run
run:
	WASM_BUILD_TOOLCHAIN=nightly-2020-10-05 cargo run --release -- --dev --tmp

.PHONY: build
build:
	WASM_BUILD_TOOLCHAIN=nightly-2020-10-05 cargo build --release

.PHONY: run-alice
run-alice:
	./scripts/start-alice.sh

.PHONY: run-bob
run-bob:
	./scripts/start-bob.sh

.PHONY: run-charlie
run-charlie:
	./scripts/start-charlie.sh

.PHONY: purge-chain
purge-chain:
	./scripts/purge-chain.sh