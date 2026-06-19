import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});

export const arbitrumIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'ArbitrumIngressEgress.ChannelRejectionRequestReceived',
  arbitrumIngressEgressChannelRejectionRequestReceived,
);
