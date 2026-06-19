import { z } from 'zod';
import { cfChainsDotPolkadotTransactionId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: hexString,
  transactionRef: cfChainsDotPolkadotTransactionId,
});

export const polkadotBroadcasterBroadcastSuccessEvent = defineEvent(
  'PolkadotBroadcaster.BroadcastSuccess',
  polkadotBroadcasterBroadcastSuccess,
);
