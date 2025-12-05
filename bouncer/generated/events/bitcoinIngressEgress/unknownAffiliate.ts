import { z } from 'zod';
import { accountId } from '../common';

export const bitcoinIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
