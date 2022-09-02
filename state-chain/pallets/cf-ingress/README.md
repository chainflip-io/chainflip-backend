# Chainflip Ingress Pallet

This pallet provides an interface to the Chainflip Engine to allow for the witnessing of incoming amounts to external chains.

Based on the `Intent` associated with the address the funds were witnessed on, funds are routed to the appropriate pallet for further processing. Ingress funds are either intended to be swapped or used for liquidity provision.