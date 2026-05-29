import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const transactionPaymentTransactionFeePaid = z.object({
  who: accountId,
  actualFee: numberOrHex,
  tip: numberOrHex,
});

export const transactionPaymentTransactionFeePaidEvent = defineEvent(
  'TransactionPayment.TransactionFeePaid',
  transactionPaymentTransactionFeePaid,
);
