import { z } from 'zod';
import { accountId } from '../common';

export const polkadotIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});
