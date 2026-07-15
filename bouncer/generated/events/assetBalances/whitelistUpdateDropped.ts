import { z } from 'zod';
import {
  accountId,
  palletCfAssetBalancesWhitelistWhitelistChangeForeignChainAddress,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesWhitelistUpdateDropped = z.object({
  accountId,
  change: palletCfAssetBalancesWhitelistWhitelistChangeForeignChainAddress,
});

export const assetBalancesWhitelistUpdateDroppedEvent = defineEvent(
  'AssetBalances.WhitelistUpdateDropped',
  assetBalancesWhitelistUpdateDropped,
);
