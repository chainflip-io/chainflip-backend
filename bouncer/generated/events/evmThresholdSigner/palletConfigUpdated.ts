import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';

export const evmThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});
