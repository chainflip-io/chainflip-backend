import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});

export const ethereumVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'EthereumVault.ActivationTxFailedAwaitingGovernance',
  ethereumVaultActivationTxFailedAwaitingGovernance,
);
