import { z } from 'zod';
import { palletCfThresholdSignaturePalletConfigUpdate } from '../common';

export const polkadotThresholdSignerPalletConfigUpdated = z.object({
  update: palletCfThresholdSignaturePalletConfigUpdate,
});
