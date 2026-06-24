import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const arbitrumIngressEgressUnknownAffiliateEvent = defineEvent(
  'ArbitrumIngressEgress.UnknownAffiliate',
  arbitrumIngressEgressUnknownAffiliate,
);
