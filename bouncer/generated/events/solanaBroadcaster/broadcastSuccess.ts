import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: hexString,
  transactionRef: hexString,
});

export const solanaBroadcasterBroadcastSuccessEvent = defineEvent(
  'SolanaBroadcaster.BroadcastSuccess',
  solanaBroadcasterBroadcastSuccess,
);
