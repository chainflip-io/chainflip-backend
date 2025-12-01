import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const assethubBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});
