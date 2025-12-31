import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const solanaBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});
