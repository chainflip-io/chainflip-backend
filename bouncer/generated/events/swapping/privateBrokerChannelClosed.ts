import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const swappingPrivateBrokerChannelClosed = z.object({
  brokerId: accountId,
  channelId: numberOrHex,
});
