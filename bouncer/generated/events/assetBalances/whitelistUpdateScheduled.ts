import { z } from 'zod';
import {
  accountId,
  numberOrHex,
  palletCfAssetBalancesWhitelistWhitelistChangeForeignChainAddress,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesWhitelistUpdateScheduled = z.object({
  accountId,
  change: palletCfAssetBalancesWhitelistWhitelistChangeForeignChainAddress,
  applyAt: numberOrHex,
});

export const assetBalancesWhitelistUpdateScheduledEvent = defineEvent(
  'AssetBalances.WhitelistUpdateScheduled',
  assetBalancesWhitelistUpdateScheduled,
);
