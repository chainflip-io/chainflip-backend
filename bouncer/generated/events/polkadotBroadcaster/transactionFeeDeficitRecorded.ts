import { z } from 'zod';
import { hexString, numberOrHex } from '../common';

export const polkadotBroadcasterTransactionFeeDeficitRecorded = z.object({
  beneficiary: hexString,
  amount: numberOrHex,
});
