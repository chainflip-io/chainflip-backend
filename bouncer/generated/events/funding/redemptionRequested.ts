import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingRedemptionRequested = z.object({
  accountId,
  amount: numberOrHex,
  broadcastId: z.number(),
  expiryTime: numberOrHex,
});

export const fundingRedemptionRequestedEvent = defineEvent(
  'Funding.RedemptionRequested',
  fundingRedemptionRequested,
);
