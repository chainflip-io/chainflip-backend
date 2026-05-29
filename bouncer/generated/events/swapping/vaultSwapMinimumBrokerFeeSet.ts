import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingVaultSwapMinimumBrokerFeeSet = z.object({
  brokerId: accountId,
  minimumFeeBps: z.number(),
});

export const swappingVaultSwapMinimumBrokerFeeSetEvent = defineEvent(
  'Swapping.VaultSwapMinimumBrokerFeeSet',
  swappingVaultSwapMinimumBrokerFeeSet,
);
