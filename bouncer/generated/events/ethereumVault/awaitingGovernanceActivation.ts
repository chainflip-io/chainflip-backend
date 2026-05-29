import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumVaultAwaitingGovernanceActivation = z.object({
  newPublicKey: cfChainsEvmAggKey,
});

export const ethereumVaultAwaitingGovernanceActivationEvent = defineEvent(
  'EthereumVault.AwaitingGovernanceActivation',
  ethereumVaultAwaitingGovernanceActivation,
);
