import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});

export const bscVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'BscVault.ActivationTxFailedAwaitingGovernance',
  bscVaultActivationTxFailedAwaitingGovernance,
);
