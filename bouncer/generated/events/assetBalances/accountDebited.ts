import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesAccountDebited = z.object({
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amountDebited: numberOrHex,
  newBalance: numberOrHex,
});

export const assetBalancesAccountDebitedEvent = defineEvent(
  'AssetBalances.AccountDebited',
  assetBalancesAccountDebited,
);
