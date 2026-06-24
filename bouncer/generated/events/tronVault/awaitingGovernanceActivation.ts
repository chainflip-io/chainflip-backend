import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronVaultAwaitingGovernanceActivation = z.object({ newPublicKey: cfChainsEvmAggKey });

export const tronVaultAwaitingGovernanceActivationEvent = defineEvent(
  'TronVault.AwaitingGovernanceActivation',
  tronVaultAwaitingGovernanceActivation,
);
