# Chainflip Swapping Pallet

A module to manage swapping.

## Overview

The swap pallet is responsible for handling and processing swap intents. It handles the logic from the initial intent, over the sending into the AMM to kick off the egress process to send funds to the destination address. Apart from that it also exposes an interface for Relayer to interact and create swap intents. The design of the Chainflip network requires the role of a Relayer to make it possible to interact with Chainflip without the need for specialized wallet software. To achieve this we need a pallet with which a Relayer can interact to fulfill his role and kick off the ingress process for witnessing incoming transactions to a vault for the CFE and make it possible to swap assets.

## Terminology

- Relayer: A Relayer is an on-chain account responsible for requesting swap intents on the state chain. Their role is to construct and submit Swap Intent extrinsic to the blockchain for themselves, but mostly on behalf of end users.

- Swap: The process of exchanging one asset into another one.