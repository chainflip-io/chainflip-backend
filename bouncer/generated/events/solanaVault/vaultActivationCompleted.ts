import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaVaultVaultActivationCompleted = z.null();

export const solanaVaultVaultActivationCompletedEvent = defineEvent(
  'SolanaVault.VaultActivationCompleted',
  solanaVaultVaultActivationCompleted,
);
