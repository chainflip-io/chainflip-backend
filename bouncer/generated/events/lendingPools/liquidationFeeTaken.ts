import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLiquidationFeeTaken = z.object({
  loanId: numberOrHex,
  poolFee: numberOrHex,
  networkFee: numberOrHex,
  brokerFee: numberOrHex,
});

export const lendingPoolsLiquidationFeeTakenEvent = defineEvent(
  'LendingPools.LiquidationFeeTaken',
  lendingPoolsLiquidationFeeTaken,
);
