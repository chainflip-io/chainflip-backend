import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronVaultVaultActivationCompleted = z.null();

export const tronVaultVaultActivationCompletedEvent = defineEvent(
  'TronVault.VaultActivationCompleted',
  tronVaultVaultActivationCompleted,
);
