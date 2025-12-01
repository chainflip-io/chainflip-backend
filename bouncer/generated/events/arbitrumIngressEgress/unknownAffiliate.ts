import { z } from 'zod';
import { accountId } from '../common';

export const arbitrumIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
