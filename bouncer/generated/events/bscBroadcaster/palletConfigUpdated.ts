import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const bscBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
