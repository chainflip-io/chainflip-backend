import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const arbitrumBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
