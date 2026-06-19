import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const solanaIngressEgressUnknownAffiliateEvent = defineEvent(
  'SolanaIngressEgress.UnknownAffiliate',
  solanaIngressEgressUnknownAffiliate,
);
