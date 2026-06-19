import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaVaultVaultRotatedExternally = hexString;

export const solanaVaultVaultRotatedExternallyEvent = defineEvent(
  'SolanaVault.VaultRotatedExternally',
  solanaVaultVaultRotatedExternally,
);
