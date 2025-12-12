import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const ethereumBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
