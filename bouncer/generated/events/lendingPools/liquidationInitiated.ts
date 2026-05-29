import { z } from 'zod';
import {
  accountId,
  numberOrHex,
  palletCfLendingPoolsGeneralLendingLiquidationType,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLiquidationInitiated = z.object({
  borrowerId: accountId,
  swaps: z.array(z.tuple([numberOrHex, z.array(numberOrHex)])),
  liquidationType: palletCfLendingPoolsGeneralLendingLiquidationType,
});

export const lendingPoolsLiquidationInitiatedEvent = defineEvent(
  'LendingPools.LiquidationInitiated',
  lendingPoolsLiquidationInitiated,
);
