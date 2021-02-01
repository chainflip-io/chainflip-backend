.PHONY: check
check:
	cargo check --release

.PHONY: test
test:
	cargo test --release --all

.PHONY: build
build:
	cargo build --release

.PHONY: update
update:
	git submodule foreach --recursive git pull origin master