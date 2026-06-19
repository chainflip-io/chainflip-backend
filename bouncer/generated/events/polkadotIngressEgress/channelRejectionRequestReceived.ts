import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: hexString,
});

export const polkadotIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'PolkadotIngressEgress.ChannelRejectionRequestReceived',
  polkadotIngressEgressChannelRejectionRequestReceived,
);
