import { z } from 'zod';
import { accountId, hexString } from '../common';

export const swappingAffiliateRegistration = z.object({
  brokerId: accountId,
  shortId: z.number(),
  withdrawalAddress: hexString,
  affiliateId: accountId,
});
