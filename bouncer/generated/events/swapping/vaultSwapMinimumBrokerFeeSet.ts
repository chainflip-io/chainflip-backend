import { z } from 'zod';
import { accountId } from '../common';

export const swappingVaultSwapMinimumBrokerFeeSet = z.object({
  brokerId: accountId,
  minimumFeeBps: z.number(),
});
