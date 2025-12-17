import { z } from 'zod';
import { palletCfValidatorPalletConfigUpdate } from '../common';

export const validatorPalletConfigUpdated = z.object({
  update: palletCfValidatorPalletConfigUpdate,
});
