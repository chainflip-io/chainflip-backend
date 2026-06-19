import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubVaultVaultRotatedExternally = hexString;

export const assethubVaultVaultRotatedExternallyEvent = defineEvent(
  'AssethubVault.VaultRotatedExternally',
  assethubVaultVaultRotatedExternally,
);
