import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const transactionPaymentTransactionFeePaid = z.object({
  who: accountId,
  actualFee: numberOrHex,
  tip: numberOrHex,
});
