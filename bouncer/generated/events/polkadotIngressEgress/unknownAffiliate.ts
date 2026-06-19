import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressUnknownAffiliate = z.object({
  brokerId: accountId,
  shortAffiliateId: z.number(),
});

export const polkadotIngressEgressUnknownAffiliateEvent = defineEvent(
  'PolkadotIngressEgress.UnknownAffiliate',
  polkadotIngressEgressUnknownAffiliate,
);
