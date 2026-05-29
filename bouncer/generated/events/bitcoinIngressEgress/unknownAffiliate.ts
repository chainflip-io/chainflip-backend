import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const bitcoinIngressEgressUnknownAffiliateEvent = defineEvent(
  'BitcoinIngressEgress.UnknownAffiliate',
  bitcoinIngressEgressUnknownAffiliate,
);
