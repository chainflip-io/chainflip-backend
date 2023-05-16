## State Chain

- [ ] Does this break CFE compatibility (API) - If yes/not sure, have you tagged relevant Engine Echidna on the PR?
- Were any changes to the genesis config of any pallets? If yes:
  - [ ] Has the Chainspec been updated accordingly?
- Have any new dependencies been added? If yes:
  - [ ] Has `Cargo.toml/std` section been updated accordingly? [Reference](https://www.notion.so/chainflip/Cargo-toml-s-std-section-95e0d5370bc74ecc99fd310bf5b21142)
- Has the external interface been changed? Have any extrinsics been updated or removed? If yes:
  - [ ] Has the runtime version been bumped accordingly (`transaction_version` and `spec_version`)
- Do the changes require a runtime upgrade? If yes:
  - [ ] Have any storage items or stored data types been modified? If yes:
    - [ ] Has the pallet's storage version been bumped and a storage migration been defined?

### New Pallets

- [ ] Has the top-level workspace `Cargo.toml` been updated?
- [ ] Has a README file been included in the pallet?
- [ ] Has the pallet-level `Cargo.toml` template been edited with pallet-specific details?
- [ ] Have all leftover pallet-template items, comments etc. been removed?
