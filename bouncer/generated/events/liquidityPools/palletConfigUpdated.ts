import { z } from 'zod';
import { palletCfPoolsPalletConfigUpdate } from '../common';

export const liquidityPoolsPalletConfigUpdated = z.object({
  update: palletCfPoolsPalletConfigUpdate,
});
