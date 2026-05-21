import { z } from 'zod';
import { accountId } from '../common';

export const tronIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
