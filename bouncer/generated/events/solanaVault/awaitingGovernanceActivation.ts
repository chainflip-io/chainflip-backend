import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaVaultAwaitingGovernanceActivation = z.object({ newPublicKey: hexString });

export const solanaVaultAwaitingGovernanceActivationEvent = defineEvent(
  'SolanaVault.AwaitingGovernanceActivation',
  solanaVaultAwaitingGovernanceActivation,
);
