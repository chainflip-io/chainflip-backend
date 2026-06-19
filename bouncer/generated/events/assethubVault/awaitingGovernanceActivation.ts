import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubVaultAwaitingGovernanceActivation = z.object({ newPublicKey: hexString });

export const assethubVaultAwaitingGovernanceActivationEvent = defineEvent(
  'AssethubVault.AwaitingGovernanceActivation',
  assethubVaultAwaitingGovernanceActivation,
);
