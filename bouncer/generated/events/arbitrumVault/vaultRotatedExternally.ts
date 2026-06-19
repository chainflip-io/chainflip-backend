import { z } from 'zod';
import { cfChainsEvmAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumVaultVaultRotatedExternally = cfChainsEvmAggKey;

export const arbitrumVaultVaultRotatedExternallyEvent = defineEvent(
  'ArbitrumVault.VaultRotatedExternally',
  arbitrumVaultVaultRotatedExternally,
);
