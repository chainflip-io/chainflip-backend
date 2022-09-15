# Chainflip Egress Pallet

This pallet manages the outward flowing of funds from the State chain.

This pallet provides API for other pallets to schedule funds to be transferred out of the chain.
Periodically this pallet will sweep all scheduled outward flow requests, and batch them to be dispatched.
