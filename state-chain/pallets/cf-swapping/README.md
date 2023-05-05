# Chainflip Swapping Pallet

A module to manage swapping.

## Overview

The swap pallet is responsible for handling and processing swaps. It handles the logic from the initial request, over the sending into the AMM to kick off the egress process to send funds to the destination address. Apart from that it also exposes an interface for Relayer to interact and open swap channels. The design of the Chainflip network requires the role of a Relayer to make it possible to interact with Chainflip without the need for specialized wallet software. To achieve this we need a pallet with which a Relayer can interact to fulfill his role and kick off the deposit process for witnessing incoming transactions to a vault for the CFE and make it possible to swap assets.

## Terminology

- Relayer: A Relayer is an on-chain account responsible for forwarding swap requests to the state chain on behalf of end users.

- Swap: The process of exchanging one asset into another one.
