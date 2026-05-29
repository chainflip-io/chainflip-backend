import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const assethubIngressEgressUnknownAffiliateEvent = defineEvent(
  'AssethubIngressEgress.UnknownAffiliate',
  assethubIngressEgressUnknownAffiliate,
);
