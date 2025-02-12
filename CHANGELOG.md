# Changelog

All notable changes included in each Chainflip release will be documented in this file.

## [1.8.2] - 2025-02-12

- Prevent blocking of fetches due to small edge case ([#5631](https://github.com/chainflip-io/chainflip-backend/pull/5631))

## [1.8.1] - 2025-02-11

### Features

- Refresh solana ingress elections instead of recreate.

### Fixes

- Remove incorrect ccm gas warning ([#5628](https://github.com/chainflip-io/chainflip-backend/issues/5628))

## [1.8.0] - 2025-02-07

### Features

- Bond brokers for private channels ([#5416](https://github.com/chainflip-io/chainflip-backend/issues/5416))
- Extend Evm Vault swaps functionality ([#5344](https://github.com/chainflip-io/chainflip-backend/issues/5344))
- Private Broker Channel Witnessing ([#5383](https://github.com/chainflip-io/chainflip-backend/issues/5383))
- New vault swap encoding RPC & bouncer test ([#5384](https://github.com/chainflip-io/chainflip-backend/issues/5384))
- EVM Vault swaps support for Broker and affiliates ([#5408](https://github.com/chainflip-io/chainflip-backend/issues/5408))
- Boost Support for Vault Swaps ([#5437](https://github.com/chainflip-io/chainflip-backend/issues/5437))
- Add boost_fee and broker_fees to events ([#5483](https://github.com/chainflip-io/chainflip-backend/issues/5483))
- Network Fee from Boost ([#5495](https://github.com/chainflip-io/chainflip-backend/issues/5495))
- Improve ccm gas calculations and refactors ([#5462](https://github.com/chainflip-io/chainflip-backend/issues/5462))
- BTC contract swap encoding ([#5311](https://github.com/chainflip-io/chainflip-backend/issues/5311))
- Handle rotation tx construction failures ([#5307](https://github.com/chainflip-io/chainflip-backend/issues/5307))
- Expose deposit transaction hash from ingress-egress-tracker ([#5320](https://github.com/chainflip-io/chainflip-backend/issues/5320))
- Solana CCM fallback ([#5316](https://github.com/chainflip-io/chainflip-backend/issues/5316))
- Witnessing btc smart contract swaps ([#5331](https://github.com/chainflip-io/chainflip-backend/issues/5331))
- Broker can encode btc smart contract call ([#5329](https://github.com/chainflip-io/chainflip-backend/issues/5329))
- Submit a slot number alongside nonce ([#5297](https://github.com/chainflip-io/chainflip-backend/issues/5297))
- Structured error return types for rpcs ([#5346](https://github.com/chainflip-io/chainflip-backend/issues/5346))
- Tainted transaction reporting ([#5310](https://github.com/chainflip-io/chainflip-backend/issues/5310))
- Engine-runner verifies gpg signature of old dylib when downloaded ([#5339](https://github.com/chainflip-io/chainflip-backend/issues/5339))
- Add/update contract swaps parameters ([#5343](https://github.com/chainflip-io/chainflip-backend/issues/5343))
- Add sol chain tracking metric ([#5366](https://github.com/chainflip-io/chainflip-backend/issues/5366))
- Support for opening/closing private broker channels ([#5361](https://github.com/chainflip-io/chainflip-backend/issues/5361))
- Update custom_rpc, runtime_api and broker api for broker level screening ([#5341](https://github.com/chainflip-io/chainflip-backend/issues/5341))
- Add version and affiliates to UTXO encoding ([#5385](https://github.com/chainflip-io/chainflip-backend/issues/5385))
- Runtime API versioning for monitoring API ([#5393](https://github.com/chainflip-io/chainflip-backend/issues/5393))
- `cf_pool_swap_rate_v3` with broker fee and DCA support ([#5386](https://github.com/chainflip-io/chainflip-backend/issues/5386))
- Add cf-parameters versioning ([#5392](https://github.com/chainflip-io/chainflip-backend/issues/5392))
- Add ability to configure DepositChannelLifetime via governance ([#5395](https://github.com/chainflip-io/chainflip-backend/issues/5395))
- Solana Vault swaps (base PR) ([#5309](https://github.com/chainflip-io/chainflip-backend/issues/5309))
- Affiliate Broker Registration and Lookup for Vault Swaps ([#5389](https://github.com/chainflip-io/chainflip-backend/issues/5389))
- Btc vault swap bouncer test can open a private channel ([#5410](https://github.com/chainflip-io/chainflip-backend/issues/5410))
- Deposit channel recycling for Solana ([#5411](https://github.com/chainflip-io/chainflip-backend/issues/5411))
- Custom runtime api versioning + struct versioning ([#5415](https://github.com/chainflip-io/chainflip-backend/issues/5415))
- Btc vault swap with affiliates ([#5413](https://github.com/chainflip-io/chainflip-backend/issues/5413))
- Allow for providing params to ElectoralSystem test checks ([#5441](https://github.com/chainflip-io/chainflip-backend/issues/5441))
- Try-state hook for elections pallet ([#5440](https://github.com/chainflip-io/chainflip-backend/issues/5440))
- Solana Vault Swaps Elections ([#5355](https://github.com/chainflip-io/chainflip-backend/issues/5355))
- Add assethub to localnet ([#5443](https://github.com/chainflip-io/chainflip-backend/issues/5443))
- Check all restricted balances are above minimum when redeeming ([#5444](https://github.com/chainflip-io/chainflip-backend/issues/5444))
- Cleanup VaultStartBlockNumbers upon epoch expiry ([#5450](https://github.com/chainflip-io/chainflip-backend/issues/5450))
- Custom rpc finalized block subscriptions ([#5466](https://github.com/chainflip-io/chainflip-backend/issues/5466))
- Runtime metadata hash verification ([#5467](https://github.com/chainflip-io/chainflip-backend/issues/5467))
- Add private channel address to account info rpc ([#5489](https://github.com/chainflip-io/chainflip-backend/issues/5489))
- Move order fills api to custom rpc ([#5493](https://github.com/chainflip-io/chainflip-backend/issues/5493))
- Added affiliates to cf_account_info rpc ([#5491](https://github.com/chainflip-io/chainflip-backend/issues/5491))
- Vault swap broker-api / rpc call ([#5481](https://github.com/chainflip-io/chainflip-backend/issues/5481))
- Expose vault swaps from ingress-egress-tracker ([#5463](https://github.com/chainflip-io/chainflip-backend/issues/5463))
- Add expires_at field to request swap parameter encoding ([#5506](https://github.com/chainflip-io/chainflip-backend/issues/5506))
- Min network fee ([#5518](https://github.com/chainflip-io/chainflip-backend/issues/5518))
- Support backwards compatible vault_swap_requests ([#5514](https://github.com/chainflip-io/chainflip-backend/issues/5514))
- Vault swap rpc call for building EVM call ([#5509](https://github.com/chainflip-io/chainflip-backend/issues/5509))
- Add bond to broker cf_account_info ([#5532](https://github.com/chainflip-io/chainflip-backend/issues/5532))
- Check if node is bonded before deregistering ([#5529](https://github.com/chainflip-io/chainflip-backend/issues/5529))
- Add fetch SOL for Solana Vault swaps ([#5541](https://github.com/chainflip-io/chainflip-backend/issues/5541))
- Disable engine Solana fee reporting ([#5576](https://github.com/chainflip-io/chainflip-backend/issues/5576))
- Affiliate account registering and withdrawal functionality ([#5527](https://github.com/chainflip-io/chainflip-backend/issues/5527))
- Solana ccm improvements ([#5597](https://github.com/chainflip-io/chainflip-backend/issues/5597))
- `VoterApi::vote` now returns an Option. ([#5589](https://github.com/chainflip-io/chainflip-backend/issues/5589))

### Fixes

- Keyholder check should use `HistoricalActiveEpochs` ([#5325](https://github.com/chainflip-io/chainflip-backend/issues/5325))
- Don't exit if we can't remove a file that already doesn't exist ([#5328](https://github.com/chainflip-io/chainflip-backend/issues/5328))
- Pool orderbook panic ([#5510](https://github.com/chainflip-io/chainflip-backend/issues/5510))
- Scale types for pending ceremonies ([#5286](https://github.com/chainflip-io/chainflip-backend/issues/5286))
- Ignore dust underflows in order fills rpc ([#5352](https://github.com/chainflip-io/chainflip-backend/issues/5352))
- Expire all previous epochs ([#5279](https://github.com/chainflip-io/chainflip-backend/issues/5279))
- Features for test-externalities ([#5365](https://github.com/chainflip-io/chainflip-backend/issues/5365))
- Separate lp account for lp api ([#5354](https://github.com/chainflip-io/chainflip-backend/issues/5354))
- Remove is_vote_valid ([#5363](https://github.com/chainflip-io/chainflip-backend/issues/5363))
- `request_swap_deposit_address_with_affiliates` refund address is consistent with destination address type ([#5360](https://github.com/chainflip-io/chainflip-backend/issues/5360))
- Make boost detection more accurate ([#5369](https://github.com/chainflip-io/chainflip-backend/issues/5369))
- Always sweep on RPC free balance ([#5376](https://github.com/chainflip-io/chainflip-backend/issues/5376))
- Rpc subscription return types ([#5374](https://github.com/chainflip-io/chainflip-backend/issues/5374))
- Retry with search history enabled after some attempts ([#5370](https://github.com/chainflip-io/chainflip-backend/issues/5370))
- Take into account only active suspensions ([#5359](https://github.com/chainflip-io/chainflip-backend/issues/5359))
- Use correct trait object ([#5396](https://github.com/chainflip-io/chainflip-backend/issues/5396))
- Rpc error message displaying as "{error}" ([#5399](https://github.com/chainflip-io/chainflip-backend/issues/5399))
- Use ParityDbWeight instead of RocksDbWeight. ([#5404](https://github.com/chainflip-io/chainflip-backend/issues/5404))
- Upload compressed artefacts under different name 🐛 ([#5420](https://github.com/chainflip-io/chainflip-backend/issues/5420))
- Remove affiilate mappings ([#5422](https://github.com/chainflip-io/chainflip-backend/issues/5422))
- Solana get_block transaction version ([#5424](https://github.com/chainflip-io/chainflip-backend/issues/5424))
- Add `--max-runtime-instances` 🐛 ([#5431](https://github.com/chainflip-io/chainflip-backend/issues/5431))
- Polkadot 134 update fix ([#5434](https://github.com/chainflip-io/chainflip-backend/issues/5434))
- Bouncer btc client mutex for vault swaps ([#5436](https://github.com/chainflip-io/chainflip-backend/issues/5436))
- No intermediary swap amount for single-legs ([#5438](https://github.com/chainflip-io/chainflip-backend/issues/5438))
- Don't remove while iterating ([#5460](https://github.com/chainflip-io/chainflip-backend/issues/5460))
- Return correct exit code on engine error ([#5470](https://github.com/chainflip-io/chainflip-backend/issues/5470))
- Downgrade parity-scale-codec dependencies ([#5472](https://github.com/chainflip-io/chainflip-backend/issues/5472))
- Allow insecure connections with subxt ([#5479](https://github.com/chainflip-io/chainflip-backend/issues/5479))
- Cargo audit ([#5496](https://github.com/chainflip-io/chainflip-backend/issues/5496))
- Expose error detail when swap simulation fails ([#5485](https://github.com/chainflip-io/chainflip-backend/issues/5485))
- Lp_cancel_orders_batch to accepts OrderId as string/hex ([#5504](https://github.com/chainflip-io/chainflip-backend/issues/5504))
- Expire previous epochs and rename epoch duration ([#5371](https://github.com/chainflip-io/chainflip-backend/issues/5371))
- Ccm validity check for vault swaps ([#5531](https://github.com/chainflip-io/chainflip-backend/issues/5531))
- Broker get affiliates rpc ([#5536](https://github.com/chainflip-io/chainflip-backend/issues/5536))
- Ingress egress tracker refund address encoding ([#5542](https://github.com/chainflip-io/chainflip-backend/issues/5542))
- Break out of the loop if we can't refund anymore ([#5533](https://github.com/chainflip-io/chainflip-backend/issues/5533))
- Return an error for BTC vault swaps with CCM ([#5547](https://github.com/chainflip-io/chainflip-backend/issues/5547))
- Log UtxoSelectionError as warning instead of error ([#5544](https://github.com/chainflip-io/chainflip-backend/issues/5544))
- Revert solana recycling ([#5554](https://github.com/chainflip-io/chainflip-backend/issues/5554))
- Revert adding expires_at to vault swap details ([#5555](https://github.com/chainflip-io/chainflip-backend/issues/5555))
- Only apply minimum deposit check on deposit process if it hasn't been boosted ([#5553](https://github.com/chainflip-io/chainflip-backend/issues/5553))
- Add ingress-egress pallet config metadata ([#5566](https://github.com/chainflip-io/chainflip-backend/issues/5566))
- Map node communication port for polkadot localnet outside of chainflip network range ([#5568](https://github.com/chainflip-io/chainflip-backend/issues/5568))
- Retry submission on StateDiscarded error ([#5575](https://github.com/chainflip-io/chainflip-backend/issues/5575))
- Use ccm params in a partial refund ([#5582](https://github.com/chainflip-io/chainflip-backend/issues/5582))
- Disable broker-level-screening tests for boosted deposits. ([#5593](https://github.com/chainflip-io/chainflip-backend/issues/5593))
- Audit vulnerability RUSTSEC-2025-0004 ([#5601](https://github.com/chainflip-io/chainflip-backend/issues/5601))
- Ensure lp account is registered for transfers ([#5605](https://github.com/chainflip-io/chainflip-backend/issues/5605))
- Use chain-specific account type for refund params ([#5560](https://github.com/chainflip-io/chainflip-backend/issues/5560))
- No gaps in root btc source ([#5609](https://github.com/chainflip-io/chainflip-backend/issues/5609))
- Solana elections efficiency improvements ([#5585](https://github.com/chainflip-io/chainflip-backend/issues/5585))
- Make naming more consistent for request_swap_parameter_encoding ([#5615](https://github.com/chainflip-io/chainflip-backend/issues/5615))
- Add migration for delta bases ingress changes
- Corrupted state in electoral system migration
- Change_utxo not always present ([#5340](https://github.com/chainflip-io/chainflip-backend/issues/5340))

### Refactor

- Invert the sol rpc hash check conditions + remove url from error messages ([#5379](https://github.com/chainflip-io/chainflip-backend/issues/5379))
- Split out composite electoral system trait ([#5334](https://github.com/chainflip-io/chainflip-backend/issues/5334))
- Deduplicate event finding logic ([#5412](https://github.com/chainflip-io/chainflip-backend/issues/5412))
- Vault_swap should take TransactionInId ([#5409](https://github.com/chainflip-io/chainflip-backend/issues/5409))
- Move amm math into its own crate ([#5418](https://github.com/chainflip-io/chainflip-backend/issues/5418))
- Remove op codes from btc vault swap encoding ([#5465](https://github.com/chainflip-io/chainflip-backend/issues/5465))
- Move broker api subscription to custom-rpc ([#5474](https://github.com/chainflip-io/chainflip-backend/issues/5474))
- Pull out ccm decoding into function ([#5551](https://github.com/chainflip-io/chainflip-backend/issues/5551))
- Extract commonalities of ElectoralSystem and ElectoralSystemRunner into its own trait ([#5588](https://github.com/chainflip-io/chainflip-backend/issues/5588))
- Always search history for signature statuses ([#5590](https://github.com/chainflip-io/chainflip-backend/issues/5590))
- Use `EncodedAddress` for `cf_get_open_deposit_channels` ([#5540](https://github.com/chainflip-io/chainflip-backend/issues/5540))
- Rename ScheduledTxForReject to ScheduledTransactionsForRejection ([#5535](https://github.com/chainflip-io/chainflip-backend/issues/5535))

### CI

- Upgrade action version to supress deprecation warnings ⚙️ ([#5330](https://github.com/chainflip-io/chainflip-backend/issues/5330))
- Combine deploying runner machine and running benchmarks 💅 ([#5520](https://github.com/chainflip-io/chainflip-backend/issues/5520))
- Add semantic PR check 🕵🏻‍♂️ ([#5522](https://github.com/chainflip-io/chainflip-backend/issues/5522))
- Pin the version of `actionlint` 📌 ([#5523](https://github.com/chainflip-io/chainflip-backend/issues/5523))

### Testing

- Add test for new utility ([#5324](https://github.com/chainflip-io/chainflip-backend/issues/5324))
- Fix submission watcher test ([#5358](https://github.com/chainflip-io/chainflip-backend/issues/5358))
- Bouncer test for broker level screening ([#5377](https://github.com/chainflip-io/chainflip-backend/issues/5377))
- Solana's delta-based-ingress electoral System ([#5445](https://github.com/chainflip-io/chainflip-backend/issues/5445))
- Start paused retrier test ([#5464](https://github.com/chainflip-io/chainflip-backend/issues/5464))
- Update bouncer to use the Broker Api for Solana Vault Swaps ([#5525](https://github.com/chainflip-io/chainflip-backend/issues/5525))
- Bouncer vault swap fee collection test ([#5550](https://github.com/chainflip-io/chainflip-backend/issues/5550))
- Bouncer fix ccm gas limit test ([#5600](https://github.com/chainflip-io/chainflip-backend/issues/5600))
- Added check of swap requests and egress pipelines ([#5606](https://github.com/chainflip-io/chainflip-backend/issues/5606))

## [1.7.1] - 2024-11-07

### Features

- Solana chain tracking events ([#5195](https://github.com/chainflip-io/chainflip-backend/issues/5195))
- Run benchmarks on merge queue to main ([#5197](https://github.com/chainflip-io/chainflip-backend/issues/5197))
- Upload all artifacts of release builds to s3 - not just dylibs ([#5202](https://github.com/chainflip-io/chainflip-backend/issues/5202))
- Elections pallet tests ([#5186](https://github.com/chainflip-io/chainflip-backend/issues/5186))
- Dont start a new rotation if previous rotation still pending ([#5211](https://github.com/chainflip-io/chainflip-backend/issues/5211))
- Uze zip archives for artifacts on s3 ([#5225](https://github.com/chainflip-io/chainflip-backend/issues/5225))
- Added a swap_request_id to the CcmFailed event ([#5254](https://github.com/chainflip-io/chainflip-backend/issues/5254))
- Add boosted deposit lost event ([#5260](https://github.com/chainflip-io/chainflip-backend/issues/5260))
- Change broadcast timeout to be a storage value. ([#5258](https://github.com/chainflip-io/chainflip-backend/issues/5258))
- Provide ValidatorIds to ElectoralSystem::check_consensus ([#5259](https://github.com/chainflip-io/chainflip-backend/issues/5259))
- Refactor delta based ingress generics ([#5265](https://github.com/chainflip-io/chainflip-backend/issues/5265))
- Pool fills rpc subscription ([#5214](https://github.com/chainflip-io/chainflip-backend/issues/5214))
- Support use of zepter for feature checks ([#5266](https://github.com/chainflip-io/chainflip-backend/issues/5266))
- Move contract swap extrinsics into ingress-egress pallet ([#5263](https://github.com/chainflip-io/chainflip-backend/issues/5263))
- Localnet scripts to create and recreate easier ([#5284](https://github.com/chainflip-io/chainflip-backend/issues/5284))
- Add solana monitoring data ([#5277](https://github.com/chainflip-io/chainflip-backend/issues/5277))
- Remove swap and retry batch on price impact ([#5285](https://github.com/chainflip-io/chainflip-backend/issues/5285))
- Liveness electoral system ([#5278](https://github.com/chainflip-io/chainflip-backend/issues/5278))
- Minimum chunk size setting ([#5314](https://github.com/chainflip-io/chainflip-backend/issues/5314))
- Cleanup aborted broadcasts ([#5301](https://github.com/chainflip-io/chainflip-backend/issues/5301))
- Handle rotation tx construction failures ([#5307](https://github.com/chainflip-io/chainflip-backend/issues/5307))
- Expose deposit transaction hash from ingress-egress-tracker ([#5320](https://github.com/chainflip-io/chainflip-backend/issues/5320))
- Solana CCM fallback ([#5316](https://github.com/chainflip-io/chainflip-backend/issues/5316))
- Submit a slot number alongside nonce ([#5297](https://github.com/chainflip-io/chainflip-backend/issues/5297))
- Structured error return types for rpcs ([#5346](https://github.com/chainflip-io/chainflip-backend/issues/5346))
- Transaction screening ([#5310](https://github.com/chainflip-io/chainflip-backend/issues/5310))
- Engine-runner verifies gpg signature of old dylib when downloaded ([#5339](https://github.com/chainflip-io/chainflip-backend/issues/5339))
- Add sol chain tracking metric ([#5366](https://github.com/chainflip-io/chainflip-backend/issues/5366))
- Update custom_rpc, runtime_api and broker api for broker level screening ([#5341](https://github.com/chainflip-io/chainflip-backend/issues/5341))
- Runtime API versioning for monitoring API ([#5393](https://github.com/chainflip-io/chainflip-backend/issues/5393))
- `cf_pool_swap_rate_v3` with broker fee and DCA support ([#5386](https://github.com/chainflip-io/chainflip-backend/issues/5386))
- Added EVM transfer/fetch limit ([#5237](https://github.com/chainflip-io/chainflip-backend/issues/5237))

### Fixes

- Make build-m2 blocking for upload artifacts 🐛 ([#5217](https://github.com/chainflip-io/chainflip-backend/issues/5217))
- Change try_from_iter into from_iter_or_default ([#5219](https://github.com/chainflip-io/chainflip-backend/issues/5219))
- Only call monotonic median hook if state is updated. ([#5223](https://github.com/chainflip-io/chainflip-backend/issues/5223))
- CcmFailed event erased due to storage rollback ([#5242](https://github.com/chainflip-io/chainflip-backend/issues/5242))
- Decrease broadcast timeout constants for sol and arb ([#5228](https://github.com/chainflip-io/chainflip-backend/issues/5228))
- Workflow name should be berghain ([#5246](https://github.com/chainflip-io/chainflip-backend/issues/5246))
- Triple ledger size local sol node ([#5249](https://github.com/chainflip-io/chainflip-backend/issues/5249))
- Allow artefact overrides 🐛 ([#5247](https://github.com/chainflip-io/chainflip-backend/issues/5247))
- Set_range_order now returns an error if no position is created ([#4540](https://github.com/chainflip-io/chainflip-backend/issues/4540))
- Correct consensus for egress witnessing (no unanimity required) ([#5274](https://github.com/chainflip-io/chainflip-backend/issues/5274))
- Force version bump endpoint ([#5280](https://github.com/chainflip-io/chainflip-backend/issues/5280))
- Filter out stale bitcoin utxos ([#5291](https://github.com/chainflip-io/chainflip-backend/issues/5291))
- Add audit exception for RUSTSEC-2024-0375 ([#5303](https://github.com/chainflip-io/chainflip-backend/issues/5303))
- Redemption amount printed not consistent in rounding ([#5296](https://github.com/chainflip-io/chainflip-backend/issues/5296))
- Only install solana if `run-job` is true 🐛 ([#5304](https://github.com/chainflip-io/chainflip-backend/issues/5304))
- Ensure unused sol deposit channels are closed ([#5312](https://github.com/chainflip-io/chainflip-backend/issues/5312))
- Increase init timeout for backup test ([#5317](https://github.com/chainflip-io/chainflip-backend/issues/5317))
- Keyholder check should use `HistoricalActiveEpochs` ([#5325](https://github.com/chainflip-io/chainflip-backend/issues/5325))
- Don't exit if we can't remove a file that already doesn't exist ([#5328](https://github.com/chainflip-io/chainflip-backend/issues/5328))
- Scale types for pending ceremonies ([#5286](https://github.com/chainflip-io/chainflip-backend/issues/5286))
- Ignore dust underflows in order fills rpc ([#5352](https://github.com/chainflip-io/chainflip-backend/issues/5352))
- Features for test-externalities ([#5365](https://github.com/chainflip-io/chainflip-backend/issues/5365))
- Separate lp account for lp api ([#5354](https://github.com/chainflip-io/chainflip-backend/issues/5354))
- Remove is_vote_valid ([#5363](https://github.com/chainflip-io/chainflip-backend/issues/5363))
- Make boost detection more accurate ([#5369](https://github.com/chainflip-io/chainflip-backend/issues/5369))
- Always sweep on RPC free balance ([#5376](https://github.com/chainflip-io/chainflip-backend/issues/5376))
- Rpc subscription return types ([#5374](https://github.com/chainflip-io/chainflip-backend/issues/5374))
- Retry with search history enabled after some attempts ([#5370](https://github.com/chainflip-io/chainflip-backend/issues/5370))
- Take into account only active suspensions ([#5359](https://github.com/chainflip-io/chainflip-backend/issues/5359))
- Change_utxo not always present ([#5340](https://github.com/chainflip-io/chainflip-backend/issues/5340))

## [1.6.7] - 2024-09-25

### Fixes

- Add missing broker fees migration.
- Close expired Solana channels.

## [1.6.6] - 2024-09-24

### Fixes

- Prevent stale utxos from being selected for egress.

## [1.6.5] - 2024-09-10

### Fixes

- Correct consensus for egress witnessing (no unanimity required)

## [1.6.4] - 2024-09-06

### Fixes

- Consensus for Change elections should not require unanimity

## [1.6.3] - 2024-09-04

### Fixes

- Decrease broadcast timeout constants for sol and arb
- More accurate compute unit limits for Solana
- Only call monotonic median hook if state is updated. ([#5223](https://github.com/chainflip-io/chainflip-backend/issues/5223))
- Remove WS endpoint from Solana settings as it's not used.

## [1.6.2] - 2024-08-30

### Fixes

- Change try_from_iter into from_iter_or_default

## [1.6.1] - 2024-08-30

### Fixes

- Downgrade log message severity for voting / retrier

## [1.6.0] - 2024-08-23

### Features

- Solana gas tweaks ([#5140](https://github.com/chainflip-io/chainflip-backend/issues/5140))
- Core DCA ([#5106](https://github.com/chainflip-io/chainflip-backend/issues/5106))
- Add DCA details to scheduled swaps subscription ([#5182](https://github.com/chainflip-io/chainflip-backend/issues/5182))
- Solana deployments ([#5188](https://github.com/chainflip-io/chainflip-backend/issues/5188))
- Charging broker fee per chunk and in USDC ([#5161](https://github.com/chainflip-io/chainflip-backend/issues/5161))
- FoK for CCM swaps ([#5187](https://github.com/chainflip-io/chainflip-backend/issues/5187))
- Return early if no client ready on retry limited requests ([#5057](https://github.com/chainflip-io/chainflip-backend/issues/5057))
- Expose zero remaining value orders from cf_pool_orders RPC ([#5042](https://github.com/chainflip-io/chainflip-backend/issues/5042))
- Upload `chainflip-engine` shared libraries to S3 💾 ([#5061](https://github.com/chainflip-io/chainflip-backend/issues/5061))
- Exclude validators with negative rep from auction ([#5062](https://github.com/chainflip-io/chainflip-backend/issues/5062))
- RPC to return available pools ([#5065](https://github.com/chainflip-io/chainflip-backend/issues/5065))
- Add boost balances to account info ([#5028](https://github.com/chainflip-io/chainflip-backend/issues/5028))
- Apicall stores signer, store current on chain key ([#5063](https://github.com/chainflip-io/chainflip-backend/issues/5063))
- Allow arbitrary accounts to be used as affiliate broker ([#5097](https://github.com/chainflip-io/chainflip-backend/issues/5097))
- Add FoK config items to `cf_swapping_environment` ([#5099](https://github.com/chainflip-io/chainflip-backend/issues/5099))
- Emit swap request first ([#5102](https://github.com/chainflip-io/chainflip-backend/issues/5102))
- Solana chain integration ([#4849](https://github.com/chainflip-io/chainflip-backend/issues/4849))
- Governance extrinsic to recover Solana Durable Nonces ([#5130](https://github.com/chainflip-io/chainflip-backend/issues/5130))
- Add maybe_ccm_limit ([#5151](https://github.com/chainflip-io/chainflip-backend/issues/5151))
- Solana resigning ([#5124](https://github.com/chainflip-io/chainflip-backend/issues/5124))
- Sol event encoding tests ([#5156](https://github.com/chainflip-io/chainflip-backend/issues/5156))
- DCA parameter validation ([#5144](https://github.com/chainflip-io/chainflip-backend/issues/5144))
- Move asset balances into asset-balances pallet (([PRO-1422](https://linear.app/chainflip/issue/PRO-1422))) ([#5083](https://github.com/chainflip-io/chainflip-backend/issues/5083))
- Bouncer can observe past events ([#5136](https://github.com/chainflip-io/chainflip-backend/issues/5136))
- Generic configurable election-based witnessing pallet ([#5060](https://github.com/chainflip-io/chainflip-backend/issues/5060))
- Added benchmark for delete vote ([#5189](https://github.com/chainflip-io/chainflip-backend/issues/5189))
- Use encoded address in ccm deposit metadata in events ([#5192](https://github.com/chainflip-io/chainflip-backend/issues/5192))
- Add the ability to close all open orders ([#5038](https://github.com/chainflip-io/chainflip-backend/issues/5038))
- Solana chain tracking events ([#5195](https://github.com/chainflip-io/chainflip-backend/issues/5195))

### Fixes

- Add missing Arbitrum block_height ([#5126](https://github.com/chainflip-io/chainflip-backend/issues/5126))
- Account deletion logic to keep boost funds into consideration ([#5059](https://github.com/chainflip-io/chainflip-backend/issues/5059))
- Grep correct spec_version ([#5070](https://github.com/chainflip-io/chainflip-backend/issues/5070))
- 3-node localnet in CI 🥽 ([#5085](https://github.com/chainflip-io/chainflip-backend/issues/5085))
- Pallet versions should match release + bump old dylib version to 1.5.1 ([#5142](https://github.com/chainflip-io/chainflip-backend/issues/5142))
- Qualify nodes based on reputation ([#5118](https://github.com/chainflip-io/chainflip-backend/issues/5118))
- Don't panic in SCC extrinsic detection (([PRO-1250](https://linear.app/chainflip/issue/PRO-1250))) ([#5146](https://github.com/chainflip-io/chainflip-backend/issues/5146))
- Use encoded addresses in refund params for events ([#5138](https://github.com/chainflip-io/chainflip-backend/issues/5138))
- Expose solana ws rpc endpoint 🐛☀️ ([#5162](https://github.com/chainflip-io/chainflip-backend/issues/5162))
- Allow unswept funds to be used in boost ([#5164](https://github.com/chainflip-io/chainflip-backend/issues/5164))

### Refactor

- Impl_pallet_safe_mode for instantiable pallets ([#5058](https://github.com/chainflip-io/chainflip-backend/issues/5058))
- Bouncer cmd for FoK swap ([#5103](https://github.com/chainflip-io/chainflip-backend/issues/5103))
- Move info to correct environment ([#5115](https://github.com/chainflip-io/chainflip-backend/issues/5115))

### Testing

- Bouncer Fill or Kill test ([#5056](https://github.com/chainflip-io/chainflip-backend/issues/5056))
- FoK 0 refund duration ([#5111](https://github.com/chainflip-io/chainflip-backend/issues/5111))
- Add Solana delta ingress witnessing test to bouncer ([#5155](https://github.com/chainflip-io/chainflip-backend/issues/5155))
- Bouncer test for DCA Swap ([#5137](https://github.com/chainflip-io/chainflip-backend/issues/5137))

## [1.5.1] - 2024-08-13

### Documentation

- Point out localnet debug file location 🕵🏻‍♂️ ([#4850](https://github.com/chainflip-io/chainflip-backend/issues/4850))
- Remove the mention of 1Password and Docker login 😅 ([#4988](https://github.com/chainflip-io/chainflip-backend/issues/4988))
- Comment on the + WitnessPeriod

### Features

- RPC lp total balances ([#4951](https://github.com/chainflip-io/chainflip-backend/issues/4951))
- Make FoK constants configurable ([#5024](https://github.com/chainflip-io/chainflip-backend/issues/5024))
- Require upgrade-check pass on merge queue ([#4878](https://github.com/chainflip-io/chainflip-backend/issues/4878))
- Run upgrade-test on commit of trigger ([#4881](https://github.com/chainflip-io/chainflip-backend/issues/4881))
- Swap rate with limit orders [([WEB-1059](https://linear.app/chainflip/issue/WEB-1059))][([WEB-1062](https://linear.app/chainflip/issue/WEB-1062))] ([#4778](https://github.com/chainflip-io/chainflip-backend/issues/4778))
- NetworkFeeTaken includes swap_id ([#4888](https://github.com/chainflip-io/chainflip-backend/issues/4888))
- Upgrade-check is a blocking check on the merge-queue ([#4899](https://github.com/chainflip-io/chainflip-backend/issues/4899))
- Swap input excludes network fee ([#4887](https://github.com/chainflip-io/chainflip-backend/issues/4887))
- Instantiate solana ([#4763](https://github.com/chainflip-io/chainflip-backend/issues/4763))
- Check binary tomls and spec version accurately on release branches ([#4927](https://github.com/chainflip-io/chainflip-backend/issues/4927))
- Run upgrade-test on push to release branches ([#4940](https://github.com/chainflip-io/chainflip-backend/issues/4940))
- Add monitoring RPCs ([#4919](https://github.com/chainflip-io/chainflip-backend/issues/4919))
- Test benchmarks in CI ([#4944](https://github.com/chainflip-io/chainflip-backend/issues/4944))
- Chain state on bouncer failure ([#4948](https://github.com/chainflip-io/chainflip-backend/issues/4948))
- Return deposit tx_hashes where possible ([#4946](https://github.com/chainflip-io/chainflip-backend/issues/4946))
- Try to submit previous btc broadcasts before our own ([#4978](https://github.com/chainflip-io/chainflip-backend/issues/4978))
- Core fill-or-kill logic ([#4950](https://github.com/chainflip-io/chainflip-backend/issues/4950))
- Scheduled swap sub price estimation fallback ([#4987](https://github.com/chainflip-io/chainflip-backend/issues/4987))
- Polkadot v1.2.5 support ([#5026](https://github.com/chainflip-io/chainflip-backend/issues/5026))
- Add fok params to SwapDepositAddressReady ([#5013](https://github.com/chainflip-io/chainflip-backend/issues/5013))
- RPC safe mode statuses ([#4967](https://github.com/chainflip-io/chainflip-backend/issues/4967))
- Scale to json decoder ([#4889](https://github.com/chainflip-io/chainflip-backend/issues/4889))
- Consistent broker api address strings ([#5030](https://github.com/chainflip-io/chainflip-backend/issues/5030))
- Smart retrier endpoint selection ([#4984](https://github.com/chainflip-io/chainflip-backend/issues/4984))
- Add primary/secondary label to RPC_RETRIER_TOTAL_REQUESTS metric ([#5015](https://github.com/chainflip-io/chainflip-backend/issues/5015))
- Add `chainflip-node` probe scripts 🩺 ([#5033](https://github.com/chainflip-io/chainflip-backend/issues/5033))
- Upload `chainflip-engine` shared libraries to S3 💾 ([#5061](https://github.com/chainflip-io/chainflip-backend/issues/5061)) ([#5067](https://github.com/chainflip-io/chainflip-backend/issues/5067))
- Expose zero remaining value orders from cf_pool_orders RPC ([#5042](https://github.com/chainflip-io/chainflip-backend/issues/5042))
- Protect against pending swaps during upgrade
- Also check PendingCcms
- Add boost balances to account info ([#5028](https://github.com/chainflip-io/chainflip-backend/issues/5028))
- Add FoK config items to `cf_swapping_environment` ([#5099](https://github.com/chainflip-io/chainflip-backend/issues/5099))
- Set Solana ChainState
- Chore: bump openssl ([#5066](https://github.com/chainflip-io/chainflip-backend/issues/5066)) ([#5068](https://github.com/chainflip-io/chainflip-backend/issues/5068))
- Feat: apicall stores signer, store current on chain key ([#5063](https://github.com/chainflip-io/chainflip-backend/issues/5063))

### Fixes

- Wait for new balance on transfer asset ([#4836](https://github.com/chainflip-io/chainflip-backend/issues/4836))
- Consistent boost fee math ([#4857](https://github.com/chainflip-io/chainflip-backend/issues/4857))
- Kill node correctly in upgrade-test ([#4848](https://github.com/chainflip-io/chainflip-backend/issues/4848))
- Use release-persa for upgrade-test ([#4863](https://github.com/chainflip-io/chainflip-backend/issues/4863))
- Add `ca-certificates` to docker images 🐛 ([#4864](https://github.com/chainflip-io/chainflip-backend/issues/4864))
- Engine-runner permissions +x ([#4866](https://github.com/chainflip-io/chainflip-backend/issues/4866))
- Upgrade test same version ([#4868](https://github.com/chainflip-io/chainflip-backend/issues/4868))
- Run Arbitrum migrations ([#4876](https://github.com/chainflip-io/chainflip-backend/issues/4876))
- Remove bad equals ([#4879](https://github.com/chainflip-io/chainflip-backend/issues/4879))
- Check version correctly ([#4871](https://github.com/chainflip-io/chainflip-backend/issues/4871))
- Add merge group for downloading ([#4885](https://github.com/chainflip-io/chainflip-backend/issues/4885))
- Restart node on compatible upgrade ([#4886](https://github.com/chainflip-io/chainflip-backend/issues/4886))
- Update sisyphos genesis hash ([#4900](https://github.com/chainflip-io/chainflip-backend/issues/4900))
- Vault activation and initialization ([#4902](https://github.com/chainflip-io/chainflip-backend/issues/4902))
- Use median for base_fee and gas_fee_multiplier for arbitrum ([#4914](https://github.com/chainflip-io/chainflip-backend/issues/4914))
- Utxo consolidation works without prev key ([#4903](https://github.com/chainflip-io/chainflip-backend/issues/4903))
- Add missing debug.log on recreate in localnet/manage.sh ([#4916](https://github.com/chainflip-io/chainflip-backend/issues/4916))
- More robust governance bouncer test ([#4912](https://github.com/chainflip-io/chainflip-backend/issues/4912))
- Benchmarks artefact name 🐛 ([#4931](https://github.com/chainflip-io/chainflip-backend/issues/4931))
- Change the dylib id so it can be found with rpath ([#4923](https://github.com/chainflip-io/chainflip-backend/issues/4923))
- Correctly refrence `berghain` bin artefact 🐛 ([#4935](https://github.com/chainflip-io/chainflip-backend/issues/4935))
- Change field name ([#4937](https://github.com/chainflip-io/chainflip-backend/issues/4937))
- Mac rpath search should use executable's directory ([#4947](https://github.com/chainflip-io/chainflip-backend/issues/4947))
- Cannot add 0 funds to boost pool ([#4953](https://github.com/chainflip-io/chainflip-backend/issues/4953))
- Package should use old dylibs based on tags ([#4966](https://github.com/chainflip-io/chainflip-backend/issues/4966))
- Btc_utxo_consolidation import ([#4968](https://github.com/chainflip-io/chainflip-backend/issues/4968))
- Print cause of settings error ([#4972](https://github.com/chainflip-io/chainflip-backend/issues/4972))
- Use correct network for version bump check ([#4977](https://github.com/chainflip-io/chainflip-backend/issues/4977))
- Don't ignore arb settings ([#4982](https://github.com/chainflip-io/chainflip-backend/issues/4982))
- Remove prewitnessed deposits migration should migrate boost status ([#4993](https://github.com/chainflip-io/chainflip-backend/issues/4993))
- Use U256 return value for asset volume ([#4986](https://github.com/chainflip-io/chainflip-backend/issues/4986))
- Eagerly prune ExtraCallData ([#4994](https://github.com/chainflip-io/chainflip-backend/issues/4994))
- Only use cache on PRs to main ([#5007](https://github.com/chainflip-io/chainflip-backend/issues/5007))
- Record_fees ([#5011](https://github.com/chainflip-io/chainflip-backend/issues/5011))
- Remove rust cache ([#5010](https://github.com/chainflip-io/chainflip-backend/issues/5010))
- Add latest chainflip-engine `dylib` and `.so` files 🐛 ([#5020](https://github.com/chainflip-io/chainflip-backend/issues/5020))
- Clean up broadcast state on success only ([#4758](https://github.com/chainflip-io/chainflip-backend/issues/4758))
- Use rpath in engine dylib ([#5031](https://github.com/chainflip-io/chainflip-backend/issues/5031))
- Ingress-egress pallet migration co-dependency ([#5032](https://github.com/chainflip-io/chainflip-backend/issues/5032))
- Rpath name on 1.4.5 dylib ([#5046](https://github.com/chainflip-io/chainflip-backend/issues/5046))
- Allow validators to deregister once they no longer have relevant key material ([#5045](https://github.com/chainflip-io/chainflip-backend/issues/5045))
- Cp .so files to /usr/lib ([#5051](https://github.com/chainflip-io/chainflip-backend/issues/5051))
- Account deletion logic to keep boost funds into consideration ([#5059](https://github.com/chainflip-io/chainflip-backend/issues/5059))
- Update dylib id
- Prewitness deposits without delaying for a block
- Ccm transfers should charge broker fee
- Make network fee migration idempotent
- Api call migrations
- Use encoded address for refund_address in events ([#5135](https://github.com/chainflip-io/chainflip-backend/issues/5135))
- Fix: Add missing Arbitrum block_height ([#5126](https://github.com/chainflip-io/chainflip-backend/issues/5126))
- Fix: grep correct spec_version ([#5070](https://github.com/chainflip-io/chainflip-backend/issues/5070)) ([#5071](https://github.com/chainflip-io/chainflip-backend/issues/5071))

### Refactor

- Rename cf_asset_balances to cf_asset_free_balances ([#4837](https://github.com/chainflip-io/chainflip-backend/issues/4837))
- Ensure crypto ready ([#4858](https://github.com/chainflip-io/chainflip-backend/issues/4858))
- Remove storage for prewitnessed deposits ([#4833](https://github.com/chainflip-io/chainflip-backend/issues/4833))
- Migrate file to new observe event ([#4907](https://github.com/chainflip-io/chainflip-backend/issues/4907))
- Remove old observeEvent and observeBadEvent ([#4958](https://github.com/chainflip-io/chainflip-backend/issues/4958))
- Multicast subscriptions ([#4980](https://github.com/chainflip-io/chainflip-backend/issues/4980))
- Bouncer cleanup ([#4901](https://github.com/chainflip-io/chainflip-backend/issues/4901))
- Move collected network fee to swapping pallet ([#5014](https://github.com/chainflip-io/chainflip-backend/issues/5014))
- Move info to correct environment ([#5115](https://github.com/chainflip-io/chainflip-backend/issues/5115))

### Testing

- Adjust btc eth ccm ([#4873](https://github.com/chainflip-io/chainflip-backend/issues/4873))
- Basic runner tests for help and version ([#4983](https://github.com/chainflip-io/chainflip-backend/issues/4983))
- Engine-runner command line args passing filters correctly ([#5001](https://github.com/chainflip-io/chainflip-backend/issues/5001))
- Bouncer Fill or Kill test ([#5056](https://github.com/chainflip-io/chainflip-backend/issues/5056))
- Add retrial mechanism for EVM broadcasts ([#4875](https://github.com/chainflip-io/chainflip-backend/issues/4875))

## [1.4.5] - 2024-07-01

### Fixes

- Fix Polkadot transaction encoding by adding the new CheckMetadataHash field to SignedExtra.

## [1.4.4] - 2024-06-28

### Fixes

- Eager pruning of Chain Tracking witness data. ([#4994](https://github.com/chainflip-io/chainflip-backend/issues/4994))
- Ensure broker fees are credited correctly. ([#5011](https://github.com/chainflip-io/chainflip-backend/issues/5011))

## [1.4.3] - 2024-06-18

### Fixes

- Print cause of settings error ([#4972](https://github.com/chainflip-io/chainflip-backend/issues/4972))

## [1.4.2] - 2024-06-03

### Features

- Add arbitrum support (([PRO-1154](https://linear.app/chainflip/issue/PRO-1154))) ([#4486](https://github.com/chainflip-io/chainflip-backend/issues/4486))
- Per chain safe mode to restrict deposits ([#4819](https://github.com/chainflip-io/chainflip-backend/issues/4819))
- Limit the number of utxos selected for egress ([#4559](https://github.com/chainflip-io/chainflip-backend/issues/4559))
- Rpc endpoint for json-encoded events ([#4610](https://github.com/chainflip-io/chainflip-backend/issues/4610))
- Update sisyphos and perseverance chainspecs ([#4621](https://github.com/chainflip-io/chainflip-backend/issues/4621)) ([#4635](https://github.com/chainflip-io/chainflip-backend/issues/4635))
- Move broker_fee_collection_test inside all_concurrent_test ([#4622](https://github.com/chainflip-io/chainflip-backend/issues/4622))
- Add broker api connection limit option ([#4643](https://github.com/chainflip-io/chainflip-backend/issues/4643))
- RPC for returning scale-encoded System events ([#4638](https://github.com/chainflip-io/chainflip-backend/issues/4638))
- Support account deletion ([#4314](https://github.com/chainflip-io/chainflip-backend/issues/4314))
- Lock `bitcoin` and `polkadot` images to a specific commit 🔒 ([#4686](https://github.com/chainflip-io/chainflip-backend/issues/4686))
- Add chainspecs of all network to docker 📜🐳 ([#4702](https://github.com/chainflip-io/chainflip-backend/issues/4702))
- Update metric's buckets values ([#4708](https://github.com/chainflip-io/chainflip-backend/issues/4708))
- Gas swaps use swapping queue ([#4697](https://github.com/chainflip-io/chainflip-backend/issues/4697))
- Continuous transfer of old utxos ([#4680](https://github.com/chainflip-io/chainflip-backend/issues/4680))
- Handle prewitness deposits ([#4698](https://github.com/chainflip-io/chainflip-backend/issues/4698))
- Emit event to help diagnose failed witnessing reports ([#4741](https://github.com/chainflip-io/chainflip-backend/issues/4741))
- Add prewitness deposit events ([#4745](https://github.com/chainflip-io/chainflip-backend/issues/4745))
- Make price impact limit configurable per pool ([#4750](https://github.com/chainflip-io/chainflip-backend/issues/4750))
- Write snapshot if try runtime check fails ([#4657](https://github.com/chainflip-io/chainflip-backend/issues/4657))
- Add pool created events ([#4761](https://github.com/chainflip-io/chainflip-backend/issues/4761))
- Authorities should be sorted pseudo-randomly ([#4752](https://github.com/chainflip-io/chainflip-backend/issues/4752))
- Cf_boost_pools_depth rpc ([#4771](https://github.com/chainflip-io/chainflip-backend/issues/4771))
- Added safe mode for ingress egress pallet ([#4779](https://github.com/chainflip-io/chainflip-backend/issues/4779))
- Cf_boost_pool_details rpc ([#4780](https://github.com/chainflip-io/chainflip-backend/issues/4780))
- Allow for single binary CFE upgrades ([#4634](https://github.com/chainflip-io/chainflip-backend/issues/4634))
- Optimistic build, streamlined ci-main ([#4806](https://github.com/chainflip-io/chainflip-backend/issues/4806))
- Try-runtime build step on dev ci ([#4807](https://github.com/chainflip-io/chainflip-backend/issues/4807))
- Better diagnostics for bouncer swaps ([#4812](https://github.com/chainflip-io/chainflip-backend/issues/4812))
- Governance extrinsic to create boost pools ([#4816](https://github.com/chainflip-io/chainflip-backend/issues/4816))
- Store boost fees explicitly + rpc ([#4818](https://github.com/chainflip-io/chainflip-backend/issues/4818))
- LP Asset rebalancing (([PRO-1259](https://linear.app/chainflip/issue/PRO-1259))) ([#4784](https://github.com/chainflip-io/chainflip-backend/issues/4784))
- Efficient Arbitrum witnessing (([PRO-1098](https://linear.app/chainflip/issue/PRO-1098))) ([#4811](https://github.com/chainflip-io/chainflip-backend/issues/4811))
- Affiliate Brokers ([#4777](https://github.com/chainflip-io/chainflip-backend/issues/4777))

### Fixes

- Start localnet using correct commit ([#4623](https://github.com/chainflip-io/chainflip-backend/issues/4623))
- Allow OldAsset to support unambiguously encoding Arb USDC and Eth USDC, while maintaining backcompat (([PRO-1237](https://linear.app/chainflip/issue/PRO-1237))) ([#4614](https://github.com/chainflip-io/chainflip-backend/issues/4614))
- Usdt should use new encoding not legacy ([#4633](https://github.com/chainflip-io/chainflip-backend/issues/4633))
- Missing feature flags ([#4639](https://github.com/chainflip-io/chainflip-backend/issues/4639))
- Change pallet ordering to prevent breakage ([#4640](https://github.com/chainflip-io/chainflip-backend/issues/4640))
- Revert changes to sisyphos chainspec ([#4641](https://github.com/chainflip-io/chainflip-backend/issues/4641))
- Return rpc error ([#4637](https://github.com/chainflip-io/chainflip-backend/issues/4637))
- More try-runtime unwraps. ([#4648](https://github.com/chainflip-io/chainflip-backend/issues/4648))
- Publish `chainflip-engine1.3` to debian packages 🐞 ([#4653](https://github.com/chainflip-io/chainflip-backend/issues/4653))
- Correct cfe-events pallet version ([#4658](https://github.com/chainflip-io/chainflip-backend/issues/4658))
- Remove unused cli command line options ([#4644](https://github.com/chainflip-io/chainflip-backend/issues/4644))
- Replace u128 with U256 ([#4656](https://github.com/chainflip-io/chainflip-backend/issues/4656))
- Remove cfe events migration ([#4671](https://github.com/chainflip-io/chainflip-backend/issues/4671))
- Update confusing runtime spec version check 🤦‍♂️ ([#4672](https://github.com/chainflip-io/chainflip-backend/issues/4672))
- Submission watcher could confuse/lose track of submissions ([#4667](https://github.com/chainflip-io/chainflip-backend/issues/4667))
- Cf_pools_environment rpc encoding ([#4674](https://github.com/chainflip-io/chainflip-backend/issues/4674))
- Rename slippage -> price impact ([#4679](https://github.com/chainflip-io/chainflip-backend/issues/4679))
- Replace sepolia usdt address ([#4683](https://github.com/chainflip-io/chainflip-backend/issues/4683))
- Add dummy benchmark for account roles pallet ([#4684](https://github.com/chainflip-io/chainflip-backend/issues/4684))
- Typo in the error message ([#4694](https://github.com/chainflip-io/chainflip-backend/issues/4694))
- Correct perseverance chainspec on main ([#4704](https://github.com/chainflip-io/chainflip-backend/issues/4704))
- Continuous adapter ([#4707](https://github.com/chainflip-io/chainflip-backend/issues/4707))
- Make benchmarks work again ([#4716](https://github.com/chainflip-io/chainflip-backend/issues/4716))
- Uncomment pools pallet migration ([#4725](https://github.com/chainflip-io/chainflip-backend/issues/4725))
- Remove misleading error ([#4726](https://github.com/chainflip-io/chainflip-backend/issues/4726))
- Migration for earned fees ([#4733](https://github.com/chainflip-io/chainflip-backend/issues/4733))
- Broker flakiness on bouncer CI ([#4736](https://github.com/chainflip-io/chainflip-backend/issues/4736))
- Update some obvious gaps / inaccuracies in the main readmes ([#4738](https://github.com/chainflip-io/chainflip-backend/issues/4738))
- Use new perseverance genesis hash ([#4748](https://github.com/chainflip-io/chainflip-backend/issues/4748))
- Set initial arb block ([#4753](https://github.com/chainflip-io/chainflip-backend/issues/4753))
- Initialise ingress egress pallet values for arbitrum ([#4762](https://github.com/chainflip-io/chainflip-backend/issues/4762))
- ([PRO-1330](https://linear.app/chainflip/issue/PRO-1330)) - Move Location::panic() to inside the function, so it outputs the caller of the fn and not of the async block poll fn. ([#4769](https://github.com/chainflip-io/chainflip-backend/issues/4769))
- Arb vault rotation migration ([#4770](https://github.com/chainflip-io/chainflip-backend/issues/4770))
- Use `ubuntu-22.04` for benchmark runner provisioning workflow 🐛 ([#4773](https://github.com/chainflip-io/chainflip-backend/issues/4773))
- Extend upgrade timeout ([#4774](https://github.com/chainflip-io/chainflip-backend/issues/4774))
- Increase concurrent timeout ([#4775](https://github.com/chainflip-io/chainflip-backend/issues/4775))
- Git fetch all before checkout ([#4776](https://github.com/chainflip-io/chainflip-backend/issues/4776))
- Dry run gets fresh runtime version (([PRO-1249](https://linear.app/chainflip/issue/PRO-1249))) ([#4669](https://github.com/chainflip-io/chainflip-backend/issues/4669))
- Dot metadata update ([#4786](https://github.com/chainflip-io/chainflip-backend/issues/4786))
- Take fee on to usdc ([#4801](https://github.com/chainflip-io/chainflip-backend/issues/4801)) ([#4804](https://github.com/chainflip-io/chainflip-backend/issues/4804))
- Sign tx with correct key during rotation ([#4794](https://github.com/chainflip-io/chainflip-backend/issues/4794))
- Don't set code red on "agg-key set by gov-key" ([#4813](https://github.com/chainflip-io/chainflip-backend/issues/4813))
- Improve container pull times ⌚️ ([#4827](https://github.com/chainflip-io/chainflip-backend/issues/4827))
- Dont delete `/sbin` dirs 😵‍💫 ([#4828](https://github.com/chainflip-io/chainflip-backend/issues/4828))
- Upgrade test permissions ([#4838](https://github.com/chainflip-io/chainflip-backend/issues/4838))
- Upgrade test to kill node correctly ([#4865](https://github.com/chainflip-io/chainflip-backend/issues/4865))
- Add `ca-certificates` to docker images 🐛 ([#4867](https://github.com/chainflip-io/chainflip-backend/issues/4867))
- Increase localnet chainflip-node request and response size 1.4 ([#4880](https://github.com/chainflip-io/chainflip-backend/issues/4880))
- Prevent liquidity saturation during fee estimation ([#4834](https://github.com/chainflip-io/chainflip-backend/issues/4834)) ([#4883](https://github.com/chainflip-io/chainflip-backend/issues/4883))
- Increase javascript memory limit for bouncer all_concurrent_tests ([#4904](https://github.com/chainflip-io/chainflip-backend/issues/4904))

### Refactor

- Move vanity names to account roles pallet ([#4719](https://github.com/chainflip-io/chainflip-backend/issues/4719))
- Autodisconnect dot clients ([#4772](https://github.com/chainflip-io/chainflip-backend/issues/4772))
- Minor cleanup of retrier code and vault pallet ([#4803](https://github.com/chainflip-io/chainflip-backend/issues/4803))
- Auto-disconnect client ([#4820](https://github.com/chainflip-io/chainflip-backend/issues/4820))
- Refactor setup ([#4825](https://github.com/chainflip-io/chainflip-backend/issues/4825))

### Documentation

- Fix a typo and minor grammar issue in PRACTICES.md ([#4695](https://github.com/chainflip-io/chainflip-backend/issues/4695))

## [1.3.2] - 2024-03-27

### Features

- Add chainspecs of all network to docker 📜🐳 ([#4705](https://github.com/chainflip-io/chainflip-backend/issues/4705))
- Remove backcompat asset encoding from rpcs (and make them line up with newer Asset encoding) ([#4710](https://github.com/chainflip-io/chainflip-backend/issues/4710)

### Fixes

- Typo in the error message ([#4694](https://github.com/chainflip-io/chainflip-backend/issues/4694))
- Cherry-pick correct chainspecs for sisyphos and perseverance ([#4703](https://github.com/chainflip-io/chainflip-backend/issues/4703))
- Continuous adapter ([#4707](https://github.com/chainflip-io/chainflip-backend/issues/4707))

## [1.3.1] - 2024-03-22

- Logging: LP-API panic reported in submission watcher ([#4664](https://github.com/chainflip-io/chainflip-backend/issues/4664))
- Remove unused CLI command line options ([#4644] (<https://github.com/chainflip-io/chainflip-backend/issues/4644>))
- Add USDT to banana mode and update code to reflect storage changes ([#4685](https://github.com/chainflip-io/chainflip-backend/issues/4685))
- Add broker api connection limit option ([#4643](https://github.com/chainflip-io/chainflip-backend/issues/4643))
- Update sisyphos and perseverance chainspecs ([#4621](https://github.com/chainflip-io/chainflip-backend/issues/4621)) ([#4635](https://github.com/chainflip-io/chainflip-backend/issues/4635))
- Update confusing runtime spec version check for releases ([#4672](https://github.com/chainflip-io/chainflip-backend/issues/4672))
- Fix: submission watcher could confuse/lose track of submissions ([#4667](https://github.com/chainflip-io/chainflip-backend/issues/4667))
- Fix: cf_pools_environment rpc encoding ([#4674](https://github.com/chainflip-io/chainflip-backend/issues/4674))
- Revert changes to sisyphos chainspec ([#4641](https://github.com/chainflip-io/chainflip-backend/issues/4641))
- Allow OldAsset to support unambiguously encoding Arb USDC and Eth USDC, while maintaining backcompat (([PRO-1237](https://linear.app/chainflip/issue/PRO-1237))) ([#4614](https://github.com/chainflip-io/chainflip-backend/issues/4614))
- Remove aptly check from publish workflow ([#4650] (<https://github.com/chainflip-io/chainflip-backend/issues/4650>))
- Run CI on `nscloud` runners ([#4505](https://github.com/chainflip-io/chainflip-backend/issues/4505))
- Usdt should use new encoding not legacy ([#4633](https://github.com/chainflip-io/chainflip-backend/issues/4633))
- Publish `chainflip-engine1.3` to debian packages ([#4653](https://github.com/chainflip-io/chainflip-backend/issues/4653)) ([#4654](https://github.com/chainflip-io/chainflip-backend/issues/4654))
- Replace u128 with U256 ([#4656](https://github.com/chainflip-io/chainflip-backend/issues/4656)) ([#4663](https://github.com/chainflip-io/chainflip-backend/issues/4663))
- Update usdt address for testnet ([#4678](https://github.com/chainflip-io/chainflip-backend/issues/4678))
- Scheduled Swaps Subscription ([#4525](https://github.com/chainflip-io/chainflip-backend/issues/4525))
- Added boost_fee param ([#4469](https://github.com/chainflip-io/chainflip-backend/issues/4469))
- Added network fee to swapping environment ([#4470](https://github.com/chainflip-io/chainflip-backend/issues/4470))
- Add boost fee field to DepositChannelDetails ([#4492](https://github.com/chainflip-io/chainflip-backend/issues/4492))
- Expose ingress egress env fees in asset amounts ([#4497](https://github.com/chainflip-io/chainflip-backend/issues/4497))
- Storage migrations for signing refactor. ([#4493](https://github.com/chainflip-io/chainflip-backend/issues/4493))
- Ingress-egress-tracker on localnet startup ([#4481](https://github.com/chainflip-io/chainflip-backend/issues/4481))
- Punish nodes that do not witness in time ([#4507](https://github.com/chainflip-io/chainflip-backend/issues/4507))
- Auto pick non-breaking changes ([#4498](https://github.com/chainflip-io/chainflip-backend/issues/4498))
- Charge a fee for opening swap deposit addresses ([#4512](https://github.com/chainflip-io/chainflip-backend/issues/4512))
- More information added to AllBatchError. (([PRO-1171](https://linear.app/chainflip/issue/PRO-1171))) ([#4535](https://github.com/chainflip-io/chainflip-backend/issues/4535))
- Bump substrate deps to polkadot-sdk 1.6 ([#4504](https://github.com/chainflip-io/chainflip-backend/issues/4504))
- Extensible multi-key rotator ([#4546](https://github.com/chainflip-io/chainflip-backend/issues/4546))
- Debug logs on runtime upgrade test ([#4556](https://github.com/chainflip-io/chainflip-backend/issues/4556))
- Deploy L2 contracts upon localnet startup and send L2 TXs ([#4558](https://github.com/chainflip-io/chainflip-backend/issues/4558))
- Add broker info [([WEB-925](https://linear.app/chainflip/issue/WEB-925))] ([#4560](https://github.com/chainflip-io/chainflip-backend/issues/4560))
- Store prewitnessed deposits with id ([#4496](https://github.com/chainflip-io/chainflip-backend/issues/4496))
- Relative Slippage Limits (([PRO-1207](https://linear.app/chainflip/issue/PRO-1207))) ([#4547](https://github.com/chainflip-io/chainflip-backend/issues/4547))
- Expose tx_hash on BroadcastSuccess event ([#4561](https://github.com/chainflip-io/chainflip-backend/issues/4561))
- Add boost lp account to bouncer and fund it on setup_swaps ([#4552](https://github.com/chainflip-io/chainflip-backend/issues/4552))
- Expose command for broker fee withdrawal ([#4581](https://github.com/chainflip-io/chainflip-backend/issues/4581))
- Add block height and deposit details to PrewitnessedDeposit ([#4606](https://github.com/chainflip-io/chainflip-backend/issues/4606))
- Introduce tx fee multiplier storage item ([#4594](https://github.com/chainflip-io/chainflip-backend/issues/4594))
- Add channel opening fee to DepositAddressReady Events ([#4609](https://github.com/chainflip-io/chainflip-backend/issues/4609))
- Use swapping queue when swapping network fee for burn ([#4584](https://github.com/chainflip-io/chainflip-backend/issues/4584))
- Reputation safe mode should not prevent backup emissions ([#4485](https://github.com/chainflip-io/chainflip-backend/issues/4485))
- Recreate debug.log in build-localnet ([#4487](https://github.com/chainflip-io/chainflip-backend/issues/4487))
- Allow redemption if balance < sum of restricted funds ([#4488](https://github.com/chainflip-io/chainflip-backend/issues/4488))
- Update `snow` package to fix audit issue ([#4506](https://github.com/chainflip-io/chainflip-backend/issues/4506))
- Nicer formatting of dot addresses and payloads ([#4511](https://github.com/chainflip-io/chainflip-backend/issues/4511))
- Downgrade upload/download artifact ([#4516](https://github.com/chainflip-io/chainflip-backend/issues/4516))
- Remove old runtime benchmarks ([#4514](https://github.com/chainflip-io/chainflip-backend/issues/4514))
- Version flag ([#4520](https://github.com/chainflip-io/chainflip-backend/issues/4520))
- Remove old failed ccms ([#4502](https://github.com/chainflip-io/chainflip-backend/issues/4502))
- Pull first ([#4526](https://github.com/chainflip-io/chainflip-backend/issues/4526))
- Re-add version update ([#4533](https://github.com/chainflip-io/chainflip-backend/issues/4533))
- Don't remove `docker-compose.yml` when network stops ([#4541](https://github.com/chainflip-io/chainflip-backend/issues/4541))
- Upgrade-test should restart the chainflip-nodes on an incompatible upgrade ([#4490](https://github.com/chainflip-io/chainflip-backend/issues/4490))
- Ensure channel open fee can be paid in benchmarks ([#4544](https://github.com/chainflip-io/chainflip-backend/issues/4544))
- Activate missing migrations ([#4550](https://github.com/chainflip-io/chainflip-backend/issues/4550))
- Allow test coverage to run ([#4555](https://github.com/chainflip-io/chainflip-backend/issues/4555))
- More lenient max deposit fee in bouncer test ([#4564](https://github.com/chainflip-io/chainflip-backend/issues/4564))
- Wait for ThresholdSignature success before switching to NewKeysActivated ([#4534](https://github.com/chainflip-io/chainflip-backend/issues/4534))
- Continuous adapter (([PRO-684](https://linear.app/chainflip/issue/PRO-684))) ([#4503](https://github.com/chainflip-io/chainflip-backend/issues/4503))
- Remove bounded balance check ([#4575](https://github.com/chainflip-io/chainflip-backend/issues/4575))
- Disable try-state checks ([#4576](https://github.com/chainflip-io/chainflip-backend/issues/4576))
- Runtime upgrade state check uses AllPalletsWithoutSystem ([#4583](https://github.com/chainflip-io/chainflip-backend/issues/4583))
- Just check that the balance after is greater than before ([#4587](https://github.com/chainflip-io/chainflip-backend/issues/4587))
- Remove tight bound for ingress fee on broker test ([#4591](https://github.com/chainflip-io/chainflip-backend/issues/4591))
- Use correct pnpm deps for upgrade-test ([#4596](https://github.com/chainflip-io/chainflip-backend/issues/4596))
- Upgrade test pnpm install from commit ([#4600](https://github.com/chainflip-io/chainflip-backend/issues/4600))
- RUSTSEC-2024-0019 ([#4604](https://github.com/chainflip-io/chainflip-backend/issues/4604))
- Patch 1.2 broker test ([#4607](https://github.com/chainflip-io/chainflip-backend/issues/4607))
- Allow CLI to run in Debug mode ([#4605](https://github.com/chainflip-io/chainflip-backend/issues/4605))
- Swap subscription amounts as hex ([#4611](https://github.com/chainflip-io/chainflip-backend/issues/4611))
- Remove RpcAsset (([PRO-1187](https://linear.app/chainflip/issue/PRO-1187))) ([#4491](https://github.com/chainflip-io/chainflip-backend/issues/4491))
- Use ForeignChainAndAsset ([#4536](https://github.com/chainflip-io/chainflip-backend/issues/4536))
- Pass tx_ref as an extrinsic parameter ([#4579](https://github.com/chainflip-io/chainflip-backend/issues/4579))
- Pass out CFE incompatibility exit information to main ([#4563](https://github.com/chainflip-io/chainflip-backend/issues/4563))
- Update Solana image to latest tag ([#4574](https://github.com/chainflip-io/chainflip-backend/issues/4574))
- Ingress-egress-tracker: Add tx_ref to redis ([#4573](https://github.com/chainflip-io/chainflip-backend/issues/4573))
- Remove Arbitrum from cherry-picked code
- Add USDT ([#4628](https://github.com/chainflip-io/chainflip-backend/issues/4628))

## [1.2.1] - 2024-03-04

### Features

- Price impact protection for swaps ([#4547](https://github.com/chainflip-io/chainflip-backend/pull/4547))

## [1.2.0] - 2024-02-02

### Features

- Decouple CFE from SC events ([#4382](https://github.com/chainflip-io/chainflip-backend/issues/4382))
- Expose ingress and egress fees on events ([#4442](https://github.com/chainflip-io/chainflip-backend/issues/4442))
- BitcoinFeeInfo: Store single field, derive the rest (([PRO-1073](https://linear.app/chainflip/issue/PRO-1073))) ([#4372](https://github.com/chainflip-io/chainflip-backend/issues/4372))
- Track btc fees on success ([#4334](https://github.com/chainflip-io/chainflip-backend/issues/4334))
- Shave fees on ingress ([#4335](https://github.com/chainflip-io/chainflip-backend/issues/4335))
- Add version cmd to all bins ([#4343](https://github.com/chainflip-io/chainflip-backend/issues/4343))
- API Bins check SC compatibility ([#4342](https://github.com/chainflip-io/chainflip-backend/issues/4342))
- Enforce version is greater than release version on PRs to main ([#4351](https://github.com/chainflip-io/chainflip-backend/issues/4351))
- Spec_version of PR is greater than spec version of current release ([#4355](https://github.com/chainflip-io/chainflip-backend/issues/4355))
- End to end network upgrade github action ([#4274](https://github.com/chainflip-io/chainflip-backend/issues/4274))
- Btc utxo consolidation ([#4338](https://github.com/chainflip-io/chainflip-backend/issues/4338))
- Add chaintracking metric ([#4369](https://github.com/chainflip-io/chainflip-backend/issues/4369))
- Order_fills rpc (([PRO-1044](https://linear.app/chainflip/issue/PRO-1044))) ([#4376](https://github.com/chainflip-io/chainflip-backend/issues/4376))
- Track deposit witnesses and egress confirmations [([WEB-715](https://linear.app/chainflip/issue/WEB-715))] ([#4370](https://github.com/chainflip-io/chainflip-backend/issues/4370))
- Nightly bouncer ([#4400](https://github.com/chainflip-io/chainflip-backend/issues/4400))
- Bouncer command to check how many validator witnessed something ([#4408](https://github.com/chainflip-io/chainflip-backend/issues/4408))
- Add witness safety margin [([PRO-1059](https://linear.app/chainflip/issue/PRO-1059))] ([#4438](https://github.com/chainflip-io/chainflip-backend/issues/4438))
- Solana localnet ([#4428](https://github.com/chainflip-io/chainflip-backend/issues/4428))
- Upgrade test can be run from any commit on main ([#4421](https://github.com/chainflip-io/chainflip-backend/issues/4421))
- Improve ceremony metrics ([#4354](https://github.com/chainflip-io/chainflip-backend/issues/4354))
- Upgrade-test can work on releases ([#4453](https://github.com/chainflip-io/chainflip-backend/issues/4453))
- CFE processes SC events in the initial block on startup ([#4228](https://github.com/chainflip-io/chainflip-backend/issues/4228))
- Nightly upgrade ([#4462](https://github.com/chainflip-io/chainflip-backend/issues/4462))
- Run tests in release-mode (pragmatic approach) ([#4441](https://github.com/chainflip-io/chainflip-backend/issues/4441))
- Expose all ignored egresses in ingress-egress pallet ([#4458](https://github.com/chainflip-io/chainflip-backend/issues/4458))

### Fixes

- CFE Witnessing, use parent block metadata when decoding events ([#4331](https://github.com/chainflip-io/chainflip-backend/issues/4331))
- Build sisyphos with production profile ([#4327](https://github.com/chainflip-io/chainflip-backend/issues/4327))
- Sweeping before withdrawal ([#4337](https://github.com/chainflip-io/chainflip-backend/issues/4337))
- Changelog check 🤫 ([#4348](https://github.com/chainflip-io/chainflip-backend/issues/4348))
- Connections can become stale when reconnecting ([#4310](https://github.com/chainflip-io/chainflip-backend/issues/4310))
- Btc witnesser test failing sometimes ([#4353](https://github.com/chainflip-io/chainflip-backend/issues/4353))
- Remove pre-witnessing functionality ([#4358](https://github.com/chainflip-io/chainflip-backend/issues/4358))
- Can timeout when updating CFE version ([#4365](https://github.com/chainflip-io/chainflip-backend/issues/4365))
- Use correct runtime name for upgrade test ([#4378](https://github.com/chainflip-io/chainflip-backend/issues/4378))
- Bitcoin deposit witness code (([PRO-1078](https://linear.app/chainflip/issue/PRO-1078))) ([#4373](https://github.com/chainflip-io/chainflip-backend/issues/4373))
- Ensure api lib version is bumped ([#4387](https://github.com/chainflip-io/chainflip-backend/issues/4387))
- Cargo fmt ([#4392](https://github.com/chainflip-io/chainflip-backend/issues/4392))
- Remove meaningless bearer token ([#4397](https://github.com/chainflip-io/chainflip-backend/issues/4397))
- Avoid SCC "sparse stream" error in CLI when making requests (([PRO-998](https://linear.app/chainflip/issue/PRO-998))) ([#4393](https://github.com/chainflip-io/chainflip-backend/issues/4393))
- Bump subxt version to 0.32.1 ([#4388](https://github.com/chainflip-io/chainflip-backend/issues/4388))
- Return correct error in cf-pool pallet ([#4405](https://github.com/chainflip-io/chainflip-backend/issues/4405))
- Use existing script for upgrade job ([#4403](https://github.com/chainflip-io/chainflip-backend/issues/4403))
- Don't have conflicting redis port with localnet ([#4415](https://github.com/chainflip-io/chainflip-backend/issues/4415))
- Pool_orders rpc filters empty orders (([PRO-1039](https://linear.app/chainflip/issue/PRO-1039))) ([#4377](https://github.com/chainflip-io/chainflip-backend/issues/4377))
- Await finalisation before starting broker ([#4412](https://github.com/chainflip-io/chainflip-backend/issues/4412))
- Bump spec version command only bumps when necessary ([#4422](https://github.com/chainflip-io/chainflip-backend/issues/4422))
- Restore correct restriction on redemption expiry (([PRO-1072](https://linear.app/chainflip/issue/PRO-1072)))
- RUSTSEC-2024-0006 ([#4439](https://github.com/chainflip-io/chainflip-backend/issues/4439))
- Filter incompatible blocks (([PRO-1108](https://linear.app/chainflip/issue/PRO-1108))) ([#4432](https://github.com/chainflip-io/chainflip-backend/issues/4432))
- Use custom subxt config + nextAccountIndex ([#4440](https://github.com/chainflip-io/chainflip-backend/issues/4440))
- :bug: fixed storage version ([#4447](https://github.com/chainflip-io/chainflip-backend/issues/4447))
- Const UNFINALIZED = false; ([#4452](https://github.com/chainflip-io/chainflip-backend/issues/4452))
- Use 127.0.0.1 instead of localhost ([#4457](https://github.com/chainflip-io/chainflip-backend/issues/4457))
- Issues with merge ([#4466](https://github.com/chainflip-io/chainflip-backend/issues/4466))
- Spelling of AssetConversionError::UnsupportedAsset ([#4474](https://github.com/chainflip-io/chainflip-backend/issues/4474))
- Force egress success for benchmarks ([#4480](https://github.com/chainflip-io/chainflip-backend/issues/4480))

### Refactor

- Code cleaniness ([#4389](https://github.com/chainflip-io/chainflip-backend/issues/4389))
- Save data to redis [([WEB-761](https://linear.app/chainflip/issue/WEB-761))] ([#4399](https://github.com/chainflip-io/chainflip-backend/issues/4399))
- Improve panic behaviour ([#4391](https://github.com/chainflip-io/chainflip-backend/issues/4391))
- Give structure [([WEB-761](https://linear.app/chainflip/issue/WEB-761))] ([#4402](https://github.com/chainflip-io/chainflip-backend/issues/4402))
- Configurable expiry times [([WEB-761](https://linear.app/chainflip/issue/WEB-761))] ([#4406](https://github.com/chainflip-io/chainflip-backend/issues/4406))
- Use yargs for all try_runtime_command options ([#4423](https://github.com/chainflip-io/chainflip-backend/issues/4423))
- Remove unnecessary fields ([#4425](https://github.com/chainflip-io/chainflip-backend/issues/4425))

### Documentation

- Correct env vars ([#4416](https://github.com/chainflip-io/chainflip-backend/issues/4416))

### Testing

- Cfe event encoding ([#4429](https://github.com/chainflip-io/chainflip-backend/issues/4429))
- Latest_then adapter testing ([#4322](https://github.com/chainflip-io/chainflip-backend/issues/4322))
- Polkadot runtime update bouncer test ([#4286](https://github.com/chainflip-io/chainflip-backend/issues/4286))
- Ensure DepositIgnored(NotEnoughToPayFees) event is emitted ([#4460](https://github.com/chainflip-io/chainflip-backend/issues/4460))

## [1.1.5] - 2024-01-02

### Fixes

- Ensure all nodes consistently witness Bitcoin deposits ([#4374](https://github.com/chainflip-io/chainflip-backend/issues/4374))

## [1.1.4] - 2023-12-21

### Features

- More accurate btc fees ([#4367](https://github.com/chainflip-io/chainflip-backend/issues/4367))

## [1.1.3] - 2023-12-19

### Fixes

- Remove pre-witnessing functionality ([#4359](https://github.com/chainflip-io/chainflip-backend/issues/4359))

## [1.1.1] - 2023-12-13

### Features

- Track btc fees on success ([#4334](https://github.com/chainflip-io/chainflip-backend/issues/4334))
- Shave fees on ingress ([#4335](https://github.com/chainflip-io/chainflip-backend/issues/4335))

### Fixes

- CFE Witnessing, use parent block metadata when decoding events ([#4331](https://github.com/chainflip-io/chainflip-backend/issues/4331))
- Sweeping before withdrawal ([#4337](https://github.com/chainflip-io/chainflip-backend/issues/4337))

### Testing

- Latest_then adapter testing ([#4322](https://github.com/chainflip-io/chainflip-backend/issues/4322))

## [1.1.0] - 2023-12-07

### Features

- Handle failed vault transfers ([#4304](https://github.com/chainflip-io/chainflip-backend/issues/4304))
- Rpc to get list of supported assets. ([#4222](https://github.com/chainflip-io/chainflip-backend/issues/4222))
- Try-runtime loop ([#4267](https://github.com/chainflip-io/chainflip-backend/issues/4267))
- Original order size and accumulative fees ([#4211](https://github.com/chainflip-io/chainflip-backend/issues/4211))
- Store witness safety margins on SC ([#4260](https://github.com/chainflip-io/chainflip-backend/issues/4260))
- Simulate order book RPC ([#4269](https://github.com/chainflip-io/chainflip-backend/issues/4269))
- Upgrade_network command using prebuilt binaries ([#4280](https://github.com/chainflip-io/chainflip-backend/issues/4280))
- Try runtime in upgrade tool ([#4282](https://github.com/chainflip-io/chainflip-backend/issues/4282))
- Re-sign failed CCMs ([#4277](https://github.com/chainflip-io/chainflip-backend/issues/4277))
- ([PRO-932](https://linear.app/chainflip/issue/PRO-932)): add fee info to cf_pool_orders tpc ([#4295](https://github.com/chainflip-io/chainflip-backend/issues/4295))
- Add max swap amounts to swapping env ([#4306](https://github.com/chainflip-io/chainflip-backend/issues/4306))
- Broadcast barrier ([#4207](https://github.com/chainflip-io/chainflip-backend/issues/4207))
- Add redeeming to LP API ([#4292](https://github.com/chainflip-io/chainflip-backend/issues/4292))
- Try-runtime build to CI ([#4293](https://github.com/chainflip-io/chainflip-backend/issues/4293))
- New Lp Interfaces ([#4313](https://github.com/chainflip-io/chainflip-backend/issues/4313))
- Lp_subscribe_order_fills ([#4319](https://github.com/chainflip-io/chainflip-backend/issues/4319))

### Fixes

- Correlate new and old broadcast ids [([WEB-575](https://linear.app/chainflip/issue/WEB-575))] ([#4221](https://github.com/chainflip-io/chainflip-backend/issues/4221))
- Build sisyphos with production profile ([#4327](https://github.com/chainflip-io/chainflip-backend/issues/4327))
- Substract broker fee from swap ([#4226](https://github.com/chainflip-io/chainflip-backend/issues/4226))
- Reduce permissions of backend token ([#4265](https://github.com/chainflip-io/chainflip-backend/issues/4265))
- No dup chain tracking submissions for reorgs on unsafe stream ([#4268](https://github.com/chainflip-io/chainflip-backend/issues/4268))
- Disable spec version check for try-runtime ([#4272](https://github.com/chainflip-io/chainflip-backend/issues/4272))
- Set_fees ([#4255](https://github.com/chainflip-io/chainflip-backend/issues/4255))
- Add missing migration hook ([#4275](https://github.com/chainflip-io/chainflip-backend/issues/4275))
- Use current MAB for bond ([#4279](https://github.com/chainflip-io/chainflip-backend/issues/4279))
- Typos ([#4283](https://github.com/chainflip-io/chainflip-backend/issues/4283))
- Make restricted address storage optional ([#4285](https://github.com/chainflip-io/chainflip-backend/issues/4285))
- Bump openssl as per cargo audit ([#4289](https://github.com/chainflip-io/chainflip-backend/issues/4289))
- Correct amm migration ([#4288](https://github.com/chainflip-io/chainflip-backend/issues/4288))
- Don't remove awaiting broadcast before retry ([#4291](https://github.com/chainflip-io/chainflip-backend/issues/4291))
- Lp logs in localnet ([#4297](https://github.com/chainflip-io/chainflip-backend/issues/4297))
- Ignore extra btc vouts ([#4296](https://github.com/chainflip-io/chainflip-backend/issues/4296))
- Transaction already in chain reported as success ([#4300](https://github.com/chainflip-io/chainflip-backend/issues/4300))
- Allow deposits to different addresses in the same tx ([#4299](https://github.com/chainflip-io/chainflip-backend/issues/4299))
- Don't block unfinalised dot source when can't find events ([#4301](https://github.com/chainflip-io/chainflip-backend/issues/4301))
- Benchmark ([#4308](https://github.com/chainflip-io/chainflip-backend/issues/4308))
- Update release version on runtime upgrade ([#4281](https://github.com/chainflip-io/chainflip-backend/issues/4281))
- CLI redeem amount rounding error ([#4303](https://github.com/chainflip-io/chainflip-backend/issues/4303))
- Current release version bumped checks ([#4311](https://github.com/chainflip-io/chainflip-backend/issues/4311))
- Threshold sig migrations ([#4312](https://github.com/chainflip-io/chainflip-backend/issues/4312))
- Add migration for PendingRequestInstructions ([#4316](https://github.com/chainflip-io/chainflip-backend/issues/4316))
- Broker endpoint as env var ([#4317](https://github.com/chainflip-io/chainflip-backend/issues/4317))

### Refactor

- Keyless eth rpc client  ([#4256](https://github.com/chainflip-io/chainflip-backend/issues/4256))
- Remove generic from AssetPair types ([#4290](https://github.com/chainflip-io/chainflip-backend/issues/4290))

## [1.0.3] - 2023-12-07

### Fixes

- Use current MAB to set bond ([#4276](https://github.com/chainflip-io/chainflip-backend/issues/4276))

## [1.0.2] - 2023-11-21

### Features

- Add MAB to current auction state ([#4253](https://github.com/chainflip-io/chainflip-backend/issues/4253))
- Add SECURITY.md file ([#4263](https://github.com/chainflip-io/chainflip-backend/issues/4263))
- Get new block streams from scc after creation ([#4217](https://github.com/chainflip-io/chainflip-backend/issues/4217))
- Prewitnessing uses unfinalised sc stream ([#4220](https://github.com/chainflip-io/chainflip-backend/issues/4220))
- Improve Dot Existential Deposit Test ([#4195](https://github.com/chainflip-io/chainflip-backend/issues/4195))
- Dynamic min authority count ([#4224](https://github.com/chainflip-io/chainflip-backend/issues/4224))
- User friendly error on asset parse failure ([#4229](https://github.com/chainflip-io/chainflip-backend/issues/4229))
- CLI generate keys outputs peer id ([#4241](https://github.com/chainflip-io/chainflip-backend/issues/4241))
- Maximum Swap amount ([#4238](https://github.com/chainflip-io/chainflip-backend/issues/4238))

### Fixes

- All lp api return NumberOrHex ([#4247](https://github.com/chainflip-io/chainflip-backend/issues/4247))
- Revert restricted balances ([#4237](https://github.com/chainflip-io/chainflip-backend/issues/4237))
- Add missing spans in multisig logs ([#4239](https://github.com/chainflip-io/chainflip-backend/issues/4239))
- Remove bound addresses on account deletion ([#4244](https://github.com/chainflip-io/chainflip-backend/issues/4244))
- Remove existential deposit ([#4243](https://github.com/chainflip-io/chainflip-backend/issues/4243))
- Egress id race condition ([#4235](https://github.com/chainflip-io/chainflip-backend/issues/4235))
- Remove unwrap when getting tx receipt ([#4231](https://github.com/chainflip-io/chainflip-backend/issues/4231))
- Protect against double witnessing after safe mode ([#4254](https://github.com/chainflip-io/chainflip-backend/issues/4254))
- Runtime upgrade utils and migrations ([#4258](https://github.com/chainflip-io/chainflip-backend/issues/4258))

### Documentation

- Deposit channel lifecycle ([#4261](https://github.com/chainflip-io/chainflip-backend/issues/4261))

### Refactor

- Replace latest_finalized_hash with latest_finalized_header ([#4206](https://github.com/chainflip-io/chainflip-backend/issues/4206))
- Use btc rpc client for mempool tracker ([#4227](https://github.com/chainflip-io/chainflip-backend/issues/4227))
- Shared source inside chunked_by adapters ([#4232](https://github.com/chainflip-io/chainflip-backend/issues/4232))

## [1.0.0] - 2023-11-03

### Features

- Don't include dust btc amounts on rotation ([#4063](https://github.com/chainflip-io/chainflip-backend/issues/4063))
- Set pool fees ([#4050](https://github.com/chainflip-io/chainflip-backend/issues/4050))
- Ensure correct process termination in ingress/egress tracker ([#4101](https://github.com/chainflip-io/chainflip-backend/issues/4101))
- Ingress-egress tracking for DOT ([#4121](https://github.com/chainflip-io/chainflip-backend/issues/4121))
- Btc ingress egress tracking ([#4133](https://github.com/chainflip-io/chainflip-backend/issues/4133))
- Wait for registration before starting p2p ([#4160](https://github.com/chainflip-io/chainflip-backend/issues/4160))
- Add dry run CLI and use it in register_account_role ([#3992](https://github.com/chainflip-io/chainflip-backend/issues/3992))
- Shorter protocol id ([#3906](https://github.com/chainflip-io/chainflip-backend/issues/3906))
- New lp interface ([#3886](https://github.com/chainflip-io/chainflip-backend/issues/3886))
- More forgiving dot address parsing ([#3938](https://github.com/chainflip-io/chainflip-backend/issues/3938))
- ([PRO-474](https://linear.app/chainflip/issue/PRO-474)) broadcast safe mode ([#3902](https://github.com/chainflip-io/chainflip-backend/issues/3902))
- Backup RPC ([#3951](https://github.com/chainflip-io/chainflip-backend/issues/3951))
- Governance-pre-authorised-calls ([#3964](https://github.com/chainflip-io/chainflip-backend/issues/3964))
- Threshold signing with specific fixed key ([#3979](https://github.com/chainflip-io/chainflip-backend/issues/3979))
- Add new archive node service file ([#3937](https://github.com/chainflip-io/chainflip-backend/issues/3937))
- Qualify nodes by minimum cfe version ([#4003](https://github.com/chainflip-io/chainflip-backend/issues/4003))
- Update substrate dependency ([#3994](https://github.com/chainflip-io/chainflip-backend/issues/3994)) ([#4004](https://github.com/chainflip-io/chainflip-backend/issues/4004))
- Calculate ccm gas limit ([#3935](https://github.com/chainflip-io/chainflip-backend/issues/3935))
- [([PRO-823](https://linear.app/chainflip/issue/PRO-823))] bind-nodes-executor-to-address ([#3987](https://github.com/chainflip-io/chainflip-backend/issues/3987))
- Witnesser dispatch call filter ([#4001](https://github.com/chainflip-io/chainflip-backend/issues/4001))
- Subcribe_price and depth rpc ([#3978](https://github.com/chainflip-io/chainflip-backend/issues/3978))
- Speedy scc (([PRO-777](https://linear.app/chainflip/issue/PRO-777)) ([PRO-593](https://linear.app/chainflip/issue/PRO-593))) ([#3986](https://github.com/chainflip-io/chainflip-backend/issues/3986))
- Add initiated_at block number for egresses ([#4046](https://github.com/chainflip-io/chainflip-backend/issues/4046))
- Simple pre-witnessing ([#4056](https://github.com/chainflip-io/chainflip-backend/issues/4056))
- Size limit for CCM ([#4015](https://github.com/chainflip-io/chainflip-backend/issues/4015))
- Add WS subscription for prewitnessed swaps ([#4065](https://github.com/chainflip-io/chainflip-backend/issues/4065))
- Added logging server port setting ([#4076](https://github.com/chainflip-io/chainflip-backend/issues/4076))
- Add account roles and LP info to custom RPC ([#4089](https://github.com/chainflip-io/chainflip-backend/issues/4089))
- Add external expiry block to event [([WEB-496](https://linear.app/chainflip/issue/WEB-496))] ([#4097](https://github.com/chainflip-io/chainflip-backend/issues/4097))
- Add websocket eth subscription to deposit tracker ([#4081](https://github.com/chainflip-io/chainflip-backend/issues/4081))
- Catch dot port missing early ([#4082](https://github.com/chainflip-io/chainflip-backend/issues/4082))
- Add expiry block to liquidity channel event ([#4111](https://github.com/chainflip-io/chainflip-backend/issues/4111))
- Use snake case for lp api method names ([#4108](https://github.com/chainflip-io/chainflip-backend/issues/4108))
- Add restricted balances to AccountInfoV2 ([#4048](https://github.com/chainflip-io/chainflip-backend/issues/4048))
- Add flip balance to account info ([#4119](https://github.com/chainflip-io/chainflip-backend/issues/4119))
- Bouncer command for submitting runtime upgrades ([#4122](https://github.com/chainflip-io/chainflip-backend/issues/4122))
- Changelog config file. ([#4095](https://github.com/chainflip-io/chainflip-backend/issues/4095))
- Account_info_v2 APY ([#4112](https://github.com/chainflip-io/chainflip-backend/issues/4112))
- Required changes for multi engine release ([#4123](https://github.com/chainflip-io/chainflip-backend/issues/4123))
- Bouncer, auto bump spec version for runtime upgrades ([#4143](https://github.com/chainflip-io/chainflip-backend/issues/4143))
- Add ingress-egress documentation ([#4140](https://github.com/chainflip-io/chainflip-backend/issues/4140))
- Auto sweep earnings and accurate free balance rpc (([PRO-856](https://linear.app/chainflip/issue/PRO-856))) ([#4145](https://github.com/chainflip-io/chainflip-backend/issues/4145))
- Nested polkadot fetch ([#4006](https://github.com/chainflip-io/chainflip-backend/issues/4006))
- Verify transaction metadata ([#4078](https://github.com/chainflip-io/chainflip-backend/issues/4078))(([PRO-819](https://linear.app/chainflip/issue/PRO-819)))
- Automate compatible CFE upgrades ([#4149](https://github.com/chainflip-io/chainflip-backend/issues/4149))
- Restricted address should override bound restrictions ([#4159](https://github.com/chainflip-io/chainflip-backend/issues/4159))
- Improve environment RPC ([#4154](https://github.com/chainflip-io/chainflip-backend/issues/4154))
- Replace NumberOrHex ([#4163](https://github.com/chainflip-io/chainflip-backend/issues/4163))
- 3-node localnet ([#4086](https://github.com/chainflip-io/chainflip-backend/issues/4086))
- Update slashing values for mainnet ([#4148](https://github.com/chainflip-io/chainflip-backend/issues/4148))
- Optimistic polkadot rotation ([#4182](https://github.com/chainflip-io/chainflip-backend/issues/4182))
- Implement dry-run ([#4155](https://github.com/chainflip-io/chainflip-backend/issues/4155))
- P2p stale connections ([#4189](https://github.com/chainflip-io/chainflip-backend/issues/4189))

### Fixes

- Correct Select Median Implementation ([#3934](https://github.com/chainflip-io/chainflip-backend/issues/3934))
- Ensure existing p2p connection is removed before reconnecting ([#4045](https://github.com/chainflip-io/chainflip-backend/issues/4045))
- Limit ZMQ Buffer Size for Outgoing Messages ([#4051](https://github.com/chainflip-io/chainflip-backend/issues/4051))
- Correctly handle peer updates while waiting to reconnect ([#4052](https://github.com/chainflip-io/chainflip-backend/issues/4052))
- Correct rotation transitions on failure ([#3875](https://github.com/chainflip-io/chainflip-backend/issues/3875))
- Start ARB network and increase polkadot rpc connection limit 🐛🚀 ([#3897](https://github.com/chainflip-io/chainflip-backend/issues/3897))
- Index and hash log ([#3898](https://github.com/chainflip-io/chainflip-backend/issues/3898))
- Strictly monotonic ([#3899](https://github.com/chainflip-io/chainflip-backend/issues/3899))
- Dot decode xt ([#3904](https://github.com/chainflip-io/chainflip-backend/issues/3904))
- Is_qualified should be called for all checks ([#3910](https://github.com/chainflip-io/chainflip-backend/issues/3910))
- Broadcast success should be witnessable after a rotation ([#3921](https://github.com/chainflip-io/chainflip-backend/issues/3921))
- Log error when we try to transfer *more* than we have fetched ([#3930](https://github.com/chainflip-io/chainflip-backend/issues/3930))
- Independent witnessing startup ([#3913](https://github.com/chainflip-io/chainflip-backend/issues/3913))
- Only burn flip if non zero ([#3932](https://github.com/chainflip-io/chainflip-backend/issues/3932))
- Duplicate logging ([#3939](https://github.com/chainflip-io/chainflip-backend/issues/3939))
- Update substrate ref to use Kademlia fix ([#3941](https://github.com/chainflip-io/chainflip-backend/issues/3941))
- Tweak cli generate-keys output ([#3943](https://github.com/chainflip-io/chainflip-backend/issues/3943))
- CanonicalAssetPair encoding issue ([#3958](https://github.com/chainflip-io/chainflip-backend/issues/3958))
- Prefer finalize_signed_extrinsic in engine ([#3956](https://github.com/chainflip-io/chainflip-backend/issues/3956))
- Scale encoding skip phantom data ([#3967](https://github.com/chainflip-io/chainflip-backend/issues/3967))
- Set limit order to zero ([#3971](https://github.com/chainflip-io/chainflip-backend/issues/3971))
- Clear failed broadcasters after abort ([#3972](https://github.com/chainflip-io/chainflip-backend/issues/3972))
- Submit eip1559 transactions ([#3973](https://github.com/chainflip-io/chainflip-backend/issues/3973))
- Release build ([#3975](https://github.com/chainflip-io/chainflip-backend/issues/3975))
- Fund-redeem test ([#3982](https://github.com/chainflip-io/chainflip-backend/issues/3982))
- Set network fee to 10bps ([#4010](https://github.com/chainflip-io/chainflip-backend/issues/4010))
- Use stderr for cli messages ([#4022](https://github.com/chainflip-io/chainflip-backend/issues/4022))
- Update cfe version record even if Idle ([#4002](https://github.com/chainflip-io/chainflip-backend/issues/4002))
- Use saturating sub while calculating change amount ([#4026](https://github.com/chainflip-io/chainflip-backend/issues/4026))
- Deposit channel expiry ([#3998](https://github.com/chainflip-io/chainflip-backend/issues/3998))
- Polkadot nonce issue ([#4054](https://github.com/chainflip-io/chainflip-backend/issues/4054))
- Warn -> info ([#4060](https://github.com/chainflip-io/chainflip-backend/issues/4060))
- Loop_select conditions (([PRO-587](https://linear.app/chainflip/issue/PRO-587))) ([#4061](https://github.com/chainflip-io/chainflip-backend/issues/4061))
- Take settings backup only if migration required ([#4077](https://github.com/chainflip-io/chainflip-backend/issues/4077))
- Use percentage for eth fee history ([#4071](https://github.com/chainflip-io/chainflip-backend/issues/4071))
- Delete auction phase check for redeem cli command ([#4090](https://github.com/chainflip-io/chainflip-backend/issues/4090))
- Stop LPs without refund addresses for both assets from creating orders in a pool (([PRO-896](https://linear.app/chainflip/issue/PRO-896))) ([#4099](https://github.com/chainflip-io/chainflip-backend/issues/4099))
- Stale error handling for unsigned extrinsics (([PRO-804](https://linear.app/chainflip/issue/PRO-804))) ([#4100](https://github.com/chainflip-io/chainflip-backend/issues/4100))
- Don't abort broadcast if signers are unavailable ([#4104](https://github.com/chainflip-io/chainflip-backend/issues/4104))
- Don't egress empty all_batch calls ([#4102](https://github.com/chainflip-io/chainflip-backend/issues/4102))
- DOT swap output less than existential deposit ([#4062](https://github.com/chainflip-io/chainflip-backend/issues/4062))
- Account_info rpc address conversion ([#4144](https://github.com/chainflip-io/chainflip-backend/issues/4144))
- Add .rpc for consistency in engine settings ([#4158](https://github.com/chainflip-io/chainflip-backend/issues/4158))
- Use sc client to synchronise cfe upgrade ([#4157](https://github.com/chainflip-io/chainflip-backend/issues/4157))
- Don't ignore valid deposits when another one fails ([#4165](https://github.com/chainflip-io/chainflip-backend/issues/4165))
- Sweep broke lp returned events ([#4176](https://github.com/chainflip-io/chainflip-backend/issues/4176))
- Use `ubuntu:22.04` for docker containers 🐛 ([#4188](https://github.com/chainflip-io/chainflip-backend/issues/4188))
- Handle relative path to db ([#4164](https://github.com/chainflip-io/chainflip-backend/issues/4164))
- Change panic to bail on LP and Broker API's ([#4190](https://github.com/chainflip-io/chainflip-backend/issues/4190))

### Documentation

- Metadata fetching ([#3900](https://github.com/chainflip-io/chainflip-backend/issues/3900))
- Update funding readme with redemption restrictions ([#3914](https://github.com/chainflip-io/chainflip-backend/issues/3914))
- Amm and pools pallet ([#4005](https://github.com/chainflip-io/chainflip-backend/issues/4005))

## [0.10.0] - 2023-10-18

### Features

- Backup RPC
    Operators can now configure a backup rpc provider for the engine.
- Qualify nodes by minimum cfe version
    Operators that have not upgraded their Engines can now be excluded from Keygen ceremonies.
- Calculate ccm gas limit
    Cross chain messages now set the correct gas limit on egress.
- Executor address binding
    Accounts can now be irreversibly bound to a specific Redemption Executor.
- Witnesser dispatch call filter
    Enables selective witnessing during safe mode.
- Subcribe_price and depth rpc
    Adds AMM price and depth rpc subscriptions.
- Speedy SCC
    Extrinsic submissions via the apis no longer wait for finality.
- Add initiated_at block number for egresses
    Egress event now contains the block number at which it occurred.
- Size limit for CCM
    Limits the size of cross-chain messages.
- Required changes for multi engine release
    Adds configuration for running two Engines in parallel.

### Fixes

- Ensure existing p2p connection is removed before reconnecting
- Correctly handle peer updates while waiting to reconnect
- Clear failed broadcasters after abort
- Use stderr for cli messages
- Update cfe version record even if Idle
- State Chain client drives runtime upgrade activation
