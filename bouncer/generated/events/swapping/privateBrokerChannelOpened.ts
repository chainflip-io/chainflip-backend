import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const swappingPrivateBrokerChannelOpened = z.object({
  brokerId: accountId,
  channelId: numberOrHex,
});
