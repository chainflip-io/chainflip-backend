import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});

export const ethereumIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'EthereumIngressEgress.ChannelRejectionRequestReceived',
  ethereumIngressEgressChannelRejectionRequestReceived,
);
