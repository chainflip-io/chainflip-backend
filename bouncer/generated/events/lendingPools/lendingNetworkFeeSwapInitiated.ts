import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLendingNetworkFeeSwapInitiated = z.object({ swapRequestId: numberOrHex });

export const lendingPoolsLendingNetworkFeeSwapInitiatedEvent = defineEvent(
  'LendingPools.LendingNetworkFeeSwapInitiated',
  lendingPoolsLendingNetworkFeeSwapInitiated,
);
