import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const arbitrumBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});
