import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});

export const tronVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'TronVault.ActivationTxFailedAwaitingGovernance',
  tronVaultActivationTxFailedAwaitingGovernance,
);
