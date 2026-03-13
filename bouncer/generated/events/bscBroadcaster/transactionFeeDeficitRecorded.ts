import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const bscBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});
