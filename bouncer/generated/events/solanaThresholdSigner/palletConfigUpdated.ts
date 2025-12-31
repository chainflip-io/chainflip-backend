import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';

export const solanaThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});
