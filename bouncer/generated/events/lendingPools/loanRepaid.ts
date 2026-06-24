import { z } from 'zod';
import { numberOrHex, palletCfLendingPoolsLoanRepaidActionType } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLoanRepaid = z.object({
  loanId: numberOrHex,
  amount: numberOrHex,
  actionType: palletCfLendingPoolsLoanRepaidActionType,
});

export const lendingPoolsLoanRepaidEvent = defineEvent(
  'LendingPools.LoanRepaid',
  lendingPoolsLoanRepaid,
);
