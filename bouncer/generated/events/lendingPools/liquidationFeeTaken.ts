import { z } from 'zod';
import { numberOrHex } from '../common';

export const lendingPoolsLiquidationFeeTaken = z.object({
  loanId: numberOrHex,
  poolFee: numberOrHex,
  networkFee: numberOrHex,
  brokerFee: numberOrHex,
});
