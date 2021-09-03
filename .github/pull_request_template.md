## State Chain

- [ ] Does this break CFE compatibility (API) - If yes/not sure, have you tagged relevant Engine Echidna on the PR?
  - [ ] Type sizes on subxt (you can run the ignored test in `sc_observer.rs` with a running state chain and Nats and it will tell  you what types are missing from the runtime (`engine/src/state_chain/runtime.rs`)
- [ ] Were any changes to the genesis config of any pallets? If yes:
   - [ ] Has the Chainspec been updated accordingly?
   - [ ] Has the chainspec version been incremented?
- [ ] Is `types.json` up to date? Test this against polka js.
- [ ] Have any new dependencies been added? If yes:
   - [ ] Has `Cargo.toml/std` section been updated accordinglt?

### New Pallets

- [ ] Has the top-level workspace `Cargo.toml` been updated?
- [ ] Has a README file been included in the pallet?
- [ ] Has the pallet-level `Cargo.toml` template been edited with pallet-specific details?
- [ ] Have all leftover pallet-template items, comments etc. been removed?
- [ ] Has the pallet been added to formatting checks in CI?
