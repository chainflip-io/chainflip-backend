import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});

export const tronIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'TronIngressEgress.ChannelRejectionRequestReceived',
  tronIngressEgressChannelRejectionRequestReceived,
);
