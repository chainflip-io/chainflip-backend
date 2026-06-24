import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingPrivateBrokerChannelClosed = z.object({
  brokerId: accountId,
  channelId: numberOrHex,
});

export const swappingPrivateBrokerChannelClosedEvent = defineEvent(
  'Swapping.PrivateBrokerChannelClosed',
  swappingPrivateBrokerChannelClosed,
);
