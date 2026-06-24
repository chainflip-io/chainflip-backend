import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscVaultVaultActivationCompleted = z.null();

export const bscVaultVaultActivationCompletedEvent = defineEvent(
  'BscVault.VaultActivationCompleted',
  bscVaultVaultActivationCompleted,
);
