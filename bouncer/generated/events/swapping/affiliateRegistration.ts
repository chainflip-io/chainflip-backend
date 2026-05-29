import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingAffiliateRegistration = z.object({
  brokerId: accountId,
  shortId: z.number(),
  withdrawalAddress: hexString,
  affiliateId: accountId,
});

export const swappingAffiliateRegistrationEvent = defineEvent(
  'Swapping.AffiliateRegistration',
  swappingAffiliateRegistration,
);
