import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const tronBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});
