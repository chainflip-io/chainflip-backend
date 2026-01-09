import { z } from 'zod';
import { hexString } from '../common';

export const solanaBroadcasterBroadcastSuccess = z.object({
  broadcastId: z.number(),
  transactionOutId: hexString,
  transactionRef: hexString,
});
