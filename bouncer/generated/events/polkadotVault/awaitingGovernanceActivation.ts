import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotVaultAwaitingGovernanceActivation = z.object({ newPublicKey: hexString });

export const polkadotVaultAwaitingGovernanceActivationEvent = defineEvent(
  'PolkadotVault.AwaitingGovernanceActivation',
  polkadotVaultAwaitingGovernanceActivation,
);
