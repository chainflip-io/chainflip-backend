import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumVaultActivationTxFailedAwaitingGovernance = z.object({
  newPublicKey: cfChainsEvmAggKey,
});

export const arbitrumVaultActivationTxFailedAwaitingGovernanceEvent = defineEvent(
  'ArbitrumVault.ActivationTxFailedAwaitingGovernance',
  arbitrumVaultActivationTxFailedAwaitingGovernance,
);
