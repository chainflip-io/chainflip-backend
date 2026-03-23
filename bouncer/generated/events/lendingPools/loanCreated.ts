import { z } from 'zod';
import {
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsGeneralLendingLoanType,
} from '../common';

export const lendingPoolsLoanCreated = z.object({
  loanId: numberOrHex,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  loanType: palletCfLendingPoolsGeneralLendingLoanType,
  principalAmount: numberOrHex,
});
