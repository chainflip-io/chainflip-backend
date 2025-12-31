import { z } from 'zod';
import { numberOrHex } from '../common';

export const environmentBitcoinBlockNumberSetForVault = z.object({ blockNumber: numberOrHex });
