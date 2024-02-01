# Chainflip Funding Pallet

This pallet implements Chainflip Funding functionality.

## Overview

This pallet manages funding accounts and redeeming of funds, including:

- Receiving witnesses of events occurring in Chainflip's `StateChainGateway` Ethereum contract and updating validator's balances accordingly.
- Processing redemption requests.
- Expiring redemptions.
- Account creation when it is funded for the first time.
- Account deletion when all funds have been redeemed.

### Funding

In order to join the network and bid for a validator slot participants must fund their account with `FLIP` tokens through the `StateChainGateway` contract on Ethereum, specifying:
    1. the amount they wish to add to their account
    2. a valid account ID on the state chain network

### Redeeming

Redeeming is a bit more involved. In order to redeem available funds, active validator nodes must generate a valid threshold signature over the redemption arguments and a nonce value.

The user requests a redemption, by calling the `redeem` extrinsic (normally via the CLI).

A `RegisterRedemption` call is then broadcast to the Ethereum network. The transaction fee is paid by the authority network.

The user then needs to execute the redemption on the `StateChainGateway` contract. Executing, i.e.calling `StateChainGateway::redeem` (normally done via the Funding app UI) will emit a `Redeemed` event. This event is then witnessed on the state chain to update the validator's balance on chain.

A validator can have at most one open redemption at any given time. They must either execute the redemption, or wait for expiry until initiating a new redemption.

#### Redemption Tax

A 'Redemption Tax' is levied to compensate the network for the cost of a redemption (threshold signature, foreign call to the Eth network, etc), and to discourage frequent small redemptions. By default, for restricted addresses, the tax is paid by the redeeming account, meaning: If the account controls restricted balances, and a redemption is made to one of its restricted addresses, the tax is deducted from the restricted balance where possible.

For example: Account `A` controls 150 FLIP, and 100 FLIP are held on behalf of the restricted address `0xB`. If `0xB` requests a redemption of 50 FLIP to their restricted address, then the deduction from the restricted balance would be (50 FLIP + REDEMPTIN_TAX).

### Redemption Restrictions

Redemptions can be subject to certain rules and restrictions. The following categories apply simultaneously, that is, all of the following restrictions are checked on every redemption request.

#### Bonded Funds

Funds are bonded when a validator wins an authority slot in the auction. A validator may redeem any FLIP they own in excess of the bond.

The size of the bond depends on the outcome of the auction: The bond is set to the minimum winning auction bid.

> *Example:*
>
> *An auction resolves with 150 winners, and the lowest of the winning bids is 1,000 FLIP. An account with a balance of 1,200 FLIP would be able to redeem 200 FLIP, provided no further restrictions apply.*

#### Bidding Funds

Validators who are actively bidding in an auction cannot redeem funds. This is to prevent auction manipulation. In order to redeem any available funds, validators should redeem outside of the auction phase, or should explicitly `stop_bidding` before the auction starts.

> *Example:*
>
> *The bond is 1,000 FLIP as before, and the account balance is 1,200 FLIP. When a new auction starts, all available funds are implicitly used for bidding, and so all 1,200 FLIP are restricted and cannot be redeemed until the conclusion of the auction.*

#### Redeem Address Binding

Any account may be explicitly *bound* to a single redemption address. Henceforth, any redemption request from that account can *only redeem to this exact address*.

Note, address binding is a one-off *irreversible* operation.

> *Example:*
>
> *The account `cFc00Ld00d` is bound to the redeem address `0xdeadbeef`. This was a bad idea since it's unlikely that `cFc00Ld00d` knows the private key for `0xdeadbeef`, so his or her funds are effectively permanently locked. Do not do this.*
>
> *Example:*
>
> *A liquid staking provider wants to allow users to pool their FLIP through a smart contract on Ethereum, to then fund a validator account. They bind their validator account to the smart contract address. This binding is permanent and irrevocable, so users can now rest assured that there is no way the pooled funds can be redeemed to any other address.*

#### Executor Address Binding

Any account may be explicitly *bound* to a single executor address. Henceforth, any registered redemption from that account can *only be executed by this exact address*.

Note, address binding is a one-off *irreversible* operation.

> *Example:*
>
> *A service provider wants to integrate with Chainflip's funding/redeeming mechanism. To this end they want to ensure that they are in full control of when the redeemed funds are actually swept into the redemption account. In order to achieve this, they bind the executor address to a known Ethereum account. Without this it's technically possible for anyone to submit the `executeRedemption` call on their behalf.*

#### Restricted Balances

Certain funding *sources* are considered restricted, such that funds originating from that source can only be redeemed back to whence they came. In order to enforce this, we track the amount of funds added from restricted addresses and ensure that the account always has enough funds to cover its obligations to these addresses.

This is used primarily to enforce vesting restrictions. Some of the FLIP tokens in existence may be locked in a vesting contract that controls when the tokens can be freely accessed. In the meantime, we still would like the tokens to be used productively in the protocol. However this is not possible without restricting the funds, since otherwise the vesting restrictions can be trivially circumvented by funding an account and immediately redeeming to any other address.

> *Example:*
>
> *The address `0xabc` is marked as restricted because it is a smart contract holding FLIP for early investors.*
>
> *Imagine an account has 1,000 FLIP funded from address `0xabc` and earns a return of 10 FLIP after some period of time. Subject to other restrictions (bond etc.) those 10 FLIP can be redeemed to any address. Any more than that can only be redeemed from the restricted balance of 1,000 FLIP, and only to the originating address `0xabc`.*
