import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscVaultVaultRotatedExternally = cfChainsEvmAggKey;

export const bscVaultVaultRotatedExternallyEvent = defineEvent(
  'BscVault.VaultRotatedExternally',
  bscVaultVaultRotatedExternally,
);
