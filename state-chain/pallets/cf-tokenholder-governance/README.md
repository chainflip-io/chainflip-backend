# Chainflip Tokenholder Governance Pallet

## Overview

This pallet implements the current Chainflip token holder governance functionality. The purpose of this pallet is primarily to provide the following capabilities:

- Handle submitting Proposals by any on-chain account
- Handle backing Proposals by any on-chain account
- Handling the lifecycle of a Proposal from the voting to the enactment period
- Broadcasting a new GOV/COMM key after the Proposal has been enacted

### Mechanics

Any on-chain account is allowed to submit a Proposal for a new GOV key. Moreover, any on-chain account can back a submitted proposal with his available on-chain stake. A proposal is live for a configured voting period. If 2/3 of the total locked stake is backing a proposal, the proposal has passed the voting and goes into the enactment stage. After the enactment period is over, the proposal is getting executed.

### Side notes

- To submit a Proposal an account has to pay an extra fee. This fee is on top of the normal transaction cost and configurable in the runtime.
- If a new Proposal gets into enactment during another Proposal is already in the enactment the current Proposal gets overwritten.

## Terminology

- Proposal: A configured type of key which has specific authorizations on the configured chain instance
- Token holder: An on-chain account which has any amount of FLIP staked
- Backing: The process of supporting a proposal
- VotingPeriod: The amount of time in blocks in which a Proposal is live for backing
- EnactmentPeriod: An delay in blocks between a Proposal has passed the voting and the broadcasting of the new key
