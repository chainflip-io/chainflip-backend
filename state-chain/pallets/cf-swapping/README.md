# Chainflip Swapping Pallet

A module to manage swapping.

## Overview

TODO: Add more description about swapping

The design of the Chainflip network requires the role of a Relayer to make it possible to interact with Chainflip without the need for specialized wallet software. To achieve this we need a pallet with which a Relayer can interact to fulfill his role and kick off the ingress process for witnessing incoming transactions to a vault for the CFE and make it possible to swap assets.

## Terminology

- Relayer: A Relayer is an on-chain account responsible for requesting swap intents on the state chain. Their role is to construct and submit Swap Intent extrinsics to the blockchain for themselves, but mostly on behalf of end users.

- SwapIntent: A swap is a request to intent a trade from asset A to asset B

- SwapData: The payload of a SwapIntent
