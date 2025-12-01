import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const ethereumBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});
