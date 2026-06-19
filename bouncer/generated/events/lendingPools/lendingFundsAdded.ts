import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsSupplyAddedActionType,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLendingFundsAdded = z.object({
  lenderId: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  actionType: palletCfLendingPoolsSupplyAddedActionType,
});

export const lendingPoolsLendingFundsAddedEvent = defineEvent(
  'LendingPools.LendingFundsAdded',
  lendingPoolsLendingFundsAdded,
);
