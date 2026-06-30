import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const bscIngressEgressUnknownAffiliateEvent = defineEvent(
  'BscIngressEgress.UnknownAffiliate',
  bscIngressEgressUnknownAffiliate,
);
