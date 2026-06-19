import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumVaultAwaitingGovernanceActivation = z.object({
  newPublicKey: cfChainsEvmAggKey,
});

export const arbitrumVaultAwaitingGovernanceActivationEvent = defineEvent(
  'ArbitrumVault.AwaitingGovernanceActivation',
  arbitrumVaultAwaitingGovernanceActivation,
);
