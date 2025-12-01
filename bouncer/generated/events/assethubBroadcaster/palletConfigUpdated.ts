import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const assethubBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
