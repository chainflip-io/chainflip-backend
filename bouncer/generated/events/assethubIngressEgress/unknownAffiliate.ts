import { z } from 'zod';
import { accountId } from '../common';

export const assethubIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
