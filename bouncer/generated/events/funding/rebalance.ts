import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingRebalance = z.object({
  sourceAccountId: accountId,
  recipientAccountId: accountId,
  amount: numberOrHex,
});

export const fundingRebalanceEvent = defineEvent('Funding.Rebalance', fundingRebalance);
