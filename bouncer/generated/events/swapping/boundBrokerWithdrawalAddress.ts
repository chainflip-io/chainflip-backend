import { z } from 'zod';
import { accountId, hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingBoundBrokerWithdrawalAddress = z.object({
  broker: accountId,
  address: hexString,
});

export const swappingBoundBrokerWithdrawalAddressEvent = defineEvent(
  'Swapping.BoundBrokerWithdrawalAddress',
  swappingBoundBrokerWithdrawalAddress,
);
