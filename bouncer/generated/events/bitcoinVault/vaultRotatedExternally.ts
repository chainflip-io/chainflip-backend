import { z } from 'zod';
import { cfChainsBtcAggKey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinVaultVaultRotatedExternally = cfChainsBtcAggKey;

export const bitcoinVaultVaultRotatedExternallyEvent = defineEvent(
  'BitcoinVault.VaultRotatedExternally',
  bitcoinVaultVaultRotatedExternally,
);
