import { z } from 'zod';
import { palletCfAssetBalancesPalletConfigUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assetBalancesPalletConfigUpdated = z.object({
  update: palletCfAssetBalancesPalletConfigUpdate,
});

export const assetBalancesPalletConfigUpdatedEvent = defineEvent(
  'AssetBalances.PalletConfigUpdated',
  assetBalancesPalletConfigUpdated,
);
