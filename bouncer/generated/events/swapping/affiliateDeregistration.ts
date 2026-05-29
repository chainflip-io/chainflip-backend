import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingAffiliateDeregistration = z.object({
  brokerId: accountId,
  shortId: z.number(),
  affiliateAccountId: accountId,
});

export const swappingAffiliateDeregistrationEvent = defineEvent(
  'Swapping.AffiliateDeregistration',
  swappingAffiliateDeregistration,
);
