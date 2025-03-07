# Chainflip Trading Strategy Pallet

## Overview

This pallet can be used by LPs for creating and updating automated trading strategies.
Each strategy is assigned a unique AccountId and has balance like a regular LP account.
When LP creates a strategy, they transfer their funds to the strategy account.
When strategy's balance reaches a certain threshold, a limit order is created/updated according
to the strategy's parameters. When the order is filled, the funds are transferred back to the LP account
(within the Pools pallet), which can trigger the creation of a new limit order.
New funds can be added to the strategy an any time, however, the only way to withdraw funds from the strategy
is to close it, which will trigger cancellation of any open orders and transfer of the remaining funds back
to the LP account.
