import { z } from 'zod';
import { accountId } from '../common';

export const solanaIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
