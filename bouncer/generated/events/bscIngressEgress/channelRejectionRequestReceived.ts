import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});

export const bscIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'BscIngressEgress.ChannelRejectionRequestReceived',
  bscIngressEgressChannelRejectionRequestReceived,
);
