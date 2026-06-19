import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotVaultVaultActivationCompleted = z.null();

export const polkadotVaultVaultActivationCompletedEvent = defineEvent(
  'PolkadotVault.VaultActivationCompleted',
  polkadotVaultVaultActivationCompleted,
);
