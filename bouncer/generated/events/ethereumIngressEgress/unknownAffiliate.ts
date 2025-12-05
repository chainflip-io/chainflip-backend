import { z } from 'zod';
import { accountId } from '../common';

export const ethereumIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
