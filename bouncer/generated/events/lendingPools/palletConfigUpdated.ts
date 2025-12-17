import { z } from 'zod';
import { palletCfLendingPoolsPalletConfigUpdate } from '../common';

export const lendingPoolsPalletConfigUpdated = z.object({
  update: palletCfLendingPoolsPalletConfigUpdate,
});
