import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentBitcoinBlockNumberSetForVault = z.object({ blockNumber: numberOrHex });

export const environmentBitcoinBlockNumberSetForVaultEvent = defineEvent(
  'Environment.BitcoinBlockNumberSetForVault',
  environmentBitcoinBlockNumberSetForVault,
);
