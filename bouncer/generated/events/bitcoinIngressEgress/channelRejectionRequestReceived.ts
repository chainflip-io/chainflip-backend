import { z } from 'zod';
import { accountId, cfChainsBtcScriptPubkey } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressChannelRejectionRequestReceived = z.object({
  accountId,
  depositAddress: cfChainsBtcScriptPubkey,
});

export const bitcoinIngressEgressChannelRejectionRequestReceivedEvent = defineEvent(
  'BitcoinIngressEgress.ChannelRejectionRequestReceived',
  bitcoinIngressEgressChannelRejectionRequestReceived,
);
