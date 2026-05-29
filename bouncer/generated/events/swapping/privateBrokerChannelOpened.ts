import { z } from 'zod';
import { accountId, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingPrivateBrokerChannelOpened = z.object({
  brokerId: accountId,
  channelId: numberOrHex,
});

export const swappingPrivateBrokerChannelOpenedEvent = defineEvent(
  'Swapping.PrivateBrokerChannelOpened',
  swappingPrivateBrokerChannelOpened,
);
