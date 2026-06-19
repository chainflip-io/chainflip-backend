import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotVaultVaultRotatedExternally = hexString;

export const polkadotVaultVaultRotatedExternallyEvent = defineEvent(
  'PolkadotVault.VaultRotatedExternally',
  polkadotVaultVaultRotatedExternally,
);
