# Chainflip Tokenholder Governance Pallet

## Overview

This pallet implements the current Chainflip token holder governance functionality. The purpose of this pallet is primarily to provide the following capabilities:

- Handle submitting Proposals by any on-chain account
- Handle backing Proposals by any on-chain account
- Handling the lifecycle of a Proposal from the voting to the enactment period
- Broadcasting a new GOV/COMM key after the Proposal has been enacted

### Mechanics

Any on-chain account is allowed to submit a Proposal for a new GOV key. Moreover, any on-chain account can back a submitted proposal with his available on-chain funds. A proposal is live for a configured voting period. If 2/3 of the total locked funds are backing a proposal, the proposal has passed the voting and goes into the enactment stage. After the enactment period is over, the proposal is getting executed.

### Side notes

- To submit a Proposal an account has to pay an extra fee. This fee is on top of the normal transaction cost and configurable in the runtime.
- If a new Proposal passes the voting stage before the preceding Proposal reaches the end of the enactment phase, the preceding Proposal is replaced by the new one and a new enactment period begins.

## Terminology

- Proposal: There are two types of proposal: new governance key and new community key.
- Master Governance Key (MGK): An opaque key using cryptography compatible with the Ethereum Chain, most likely based on a Gnosis Safe. Can be used for governance actions on the Ethereum Chain and for Governance Actions on the State Chain (by runtime verification of the Ethereum signature).
- Governance Key: Each chain will have its own governance key. Governance keys in general have powers over the chain’s vault. An important distinction is that the MGK described above has additional powers over the FLIP token, and the state chain.
- Community Key: A cryptographic key controlled by the Chainflip community, used for governance oversight. Has the power to block certain governance actions like vault transfers.
- Ops Committee: The Ops committee can be set (and, by extension, revoked) by the MGK. The Ops Committee is represented by a list of substrate Public Keys (equivalent to AccountIds) can trigger State Chain Governance actions by 2/3 majority vote. (Equivalent to the current on-chain notion of Governance as implemented in the governance pallet).
- Token holder: An on-chain account which has any amount of FLIP funded
- Backing: The process of supporting a proposal
- VotingPeriod: The amount of time in blocks in which a Proposal is live for backing
- EnactmentPeriod: An delay in blocks between a Proposal has passed the voting and the broadcasting of the new key
