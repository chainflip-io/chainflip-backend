import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const bitcoinBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
