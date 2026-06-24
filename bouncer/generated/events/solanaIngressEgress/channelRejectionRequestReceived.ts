import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});

export const solanaIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'SolanaIngressEgress.ChannelRejectionRequestReceived',
  solanaIngressEgressChannelRejectionRequestReceived,
);
