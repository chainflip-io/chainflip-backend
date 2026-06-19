import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: hexString,
  transactionRef: hexString,
});

export const bitcoinBroadcasterBroadcastSuccessEvent = defineEvent(
  'BitcoinBroadcaster.BroadcastSuccess',
  bitcoinBroadcasterBroadcastSuccess,
);
