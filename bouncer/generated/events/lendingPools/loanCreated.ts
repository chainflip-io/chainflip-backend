import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsGeneralLendingLoanType,
} from '../common';

export const lendingPoolsLoanCreated = z.object({
  loanId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  loanType: palletCfLendingPoolsGeneralLendingLoanType,
  principalAmount: numberOrHex,
  brokerId: accountId.nullish(),
});
