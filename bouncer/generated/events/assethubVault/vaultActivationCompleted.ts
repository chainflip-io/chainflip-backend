import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubVaultVaultActivationCompleted = z.null();

export const assethubVaultVaultActivationCompletedEvent = defineEvent(
  'AssethubVault.VaultActivationCompleted',
  assethubVaultVaultActivationCompleted,
);
