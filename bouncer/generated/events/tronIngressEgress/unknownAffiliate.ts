import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const tronIngressEgressUnknownAffiliateEvent = defineEvent(
  'TronIngressEgress.UnknownAffiliate',
  tronIngressEgressUnknownAffiliate,
);
