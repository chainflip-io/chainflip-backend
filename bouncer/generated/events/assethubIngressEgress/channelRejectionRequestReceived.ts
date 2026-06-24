import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});

export const assethubIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'AssethubIngressEgress.ChannelRejectionRequestReceived',
  assethubIngressEgressChannelRejectionRequestReceived,
);
