.PHONY: run-tmp
run-tmp:
	SKIP_WASM_BUILD= cargo run --bin chainflip-node -- --dev --tmp -lruntime=debug

.PHONY: run
run:
	SKIP_WASM_BUILD= cargo run --bin chainflip-node -- --dev -lruntime=debug

.PHONY: toolchain
toolchain:
	./scripts/init.sh

.PHONY: build-full
build-full:
	cargo build -p chainflip-node

.PHONY: check
check:
	SKIP_WASM_BUILD= cargo check -p chainflip-node

.PHONY: build
build:
	SKIP_WASM_BUILD= cargo build -p chainflip-node

.PHONY: test
test:
	SKIP_WASM_BUILD= cargo test --all

.PHONY: check-chainflip-transactions
check-chainflip-transactions:
	SKIP_WASM_BUILD= cargo check --manifest-path pallets/chainflip-transactions/Cargo.toml

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

.PHONY: purge-chain-debug
purge-chain-debug:
	./scripts/purge-chain-debug.sh
