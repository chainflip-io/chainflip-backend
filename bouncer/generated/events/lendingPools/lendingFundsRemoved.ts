import { z } from 'zod';
import {
  accountId,
  cfPrimitivesChainsAssetsAnyAsset,
  numberOrHex,
  palletCfLendingPoolsSupplyRemovedActionType,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const lendingPoolsLendingFundsRemoved = z.object({
  lenderId: accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  unlockedAmount: numberOrHex,
  actionType: palletCfLendingPoolsSupplyRemovedActionType,
});

export const lendingPoolsLendingFundsRemovedEvent = defineEvent(
  'LendingPools.LendingFundsRemoved',
  lendingPoolsLendingFundsRemoved,
);
