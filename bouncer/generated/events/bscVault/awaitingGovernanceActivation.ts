import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscVaultAwaitingGovernanceActivation = z.object({ newPublicKey: cfChainsEvmAggKey });

export const bscVaultAwaitingGovernanceActivationEvent = defineEvent(
  'BscVault.AwaitingGovernanceActivation',
  bscVaultAwaitingGovernanceActivation,
);
