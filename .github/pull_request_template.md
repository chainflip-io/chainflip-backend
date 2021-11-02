## State Chain

- [ ] Does this break CFE compatibility (API) - If yes/not sure, have you tagged relevant Engine Echidna on the PR?
- [ ] Were any changes to the genesis config of any pallets? If yes:
   - [ ] Has the Chainspec been updated accordingly?
   - [ ] Has the chainspec version been incremented?
- [ ] Is `types.json` up to date? Test this against polka js.
- [ ] Have any new dependencies been added? If yes:
   - [ ] Has `Cargo.toml/std` section been updated accordingly? [Reference](https://www.notion.so/chainflip/Cargo-toml-s-std-section-95e0d5370bc74ecc99fd310bf5b21142)

### New Pallets

- [ ] Has the top-level workspace `Cargo.toml` been updated?
- [ ] Has a README file been included in the pallet?
- [ ] Has the pallet-level `Cargo.toml` template been edited with pallet-specific details?
- [ ] Have all leftover pallet-template items, comments etc. been removed?
- [ ] Has the pallet been added to formatting checks in CI?
