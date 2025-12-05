import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const fundingRebalance = z.object({
  sourceAccountId: accountId,
  recipientAccountId: accountId,
  amount: numberOrHex,
});
