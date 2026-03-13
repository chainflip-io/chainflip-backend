import { z } from 'zod';
import { accountId } from '../common';

export const swappingAffiliateDeregistration = z.object({
  brokerId: accountId,
  shortId: z.number(),
  affiliateAccountId: accountId,
});
