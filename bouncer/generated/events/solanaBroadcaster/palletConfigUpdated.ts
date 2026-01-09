import { z } from 'zod';
import { palletCfBroadcastPalletConfigUpdate } from '../common';

export const solanaBroadcasterPalletConfigUpdated = z.object({
  update: palletCfBroadcastPalletConfigUpdate,
});
