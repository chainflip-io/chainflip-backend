import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const ethereumIngressEgressUnknownAffiliateEvent = defineEvent(
  'EthereumIngressEgress.UnknownAffiliate',
  ethereumIngressEgressUnknownAffiliate,
);
