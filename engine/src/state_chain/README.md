# State Chain

Module responsible for any communications to/from the State Chain.

## State Chain Client

The State Chain Client acts as the raw interface to the state chain. Allowing us to query storage and submit extrinsics via the chain's RPC.

It is responsible for ensuring it submits extrinsics with a valid nonce, and to retry with a new nonce if the nonce had already been used.

## State Chain Observer

The State Chain Observer is a higher level component than the State Chain Client. It coordinates processes that require back and forth between the State Chain and the CFE.

It does this by observing *all* events emitted by the SC, but only acting on events of importance, such as `pallet_cf_vaults::Event::KeygenRequest` which is the trigger of a Key Generation ceremony.
