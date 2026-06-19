import { z } from 'zod';
import { accountId, cfPrimitivesChainsAssetsAnyAsset, numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesAccountCredited = z.object({
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amountCredited: numberOrHex,
  newBalance: numberOrHex,
});

export const assetBalancesAccountCreditedEvent = defineEvent(
  'AssetBalances.AccountCredited',
  assetBalancesAccountCredited,
);
