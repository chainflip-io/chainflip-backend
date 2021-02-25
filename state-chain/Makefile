.PHONY: init
init:
	./scripts/init.sh

.PHONY: check
check:
	SKIP_WASM_BUILD=1 cargo check -p state-chain-node --release

.PHONY: check-chainflip-transactions
check-chainflip-transactions:
	SKIP_WASM_BUILD=1 cargo check --manifest-path pallets/chainflip-transactions/Cargo.toml

.PHONY: test
test:
	SKIP_WASM_BUILD=1 cargo test --release --all

.PHONY: run
run:
	cargo run --release --bin state-chain-node -- --dev --tmp

.PHONY: build
build:
	WASM_BUILD_TOOLCHAIN=nightly cargo build -p state-chain-node --release

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