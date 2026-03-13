import { z } from 'zod';
import { accountId } from '../common';

export const bscIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
