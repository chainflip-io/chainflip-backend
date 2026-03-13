import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const tronBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
