# How to use this

1. Build the api and docgen binaries:
    `cargo build --release --bin chainflip-broker-api --bin api-docgen`

2. Generate the json schema for the api:
    `./target/release/chainflip-broker-api schema | jq > ./api/bin/chainflip-broker-api/schema.json`

3. Generate the docs:
    `./target/release/api-docgen --schema ./api/bin/chainflip-broker-api/schema.json --output ./api/bin/chainflip-broker-api/api-doc.md`

Alternatively use Make: `make broker-docs`.
