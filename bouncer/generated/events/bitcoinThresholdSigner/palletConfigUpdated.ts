import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';

export const bitcoinThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});
