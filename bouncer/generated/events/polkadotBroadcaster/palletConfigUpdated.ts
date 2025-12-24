import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const polkadotBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
