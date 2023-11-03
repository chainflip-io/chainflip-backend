# Changelog

All notable changes included in each Chainflip release will be documented in this file.

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
