import { z } from 'zod';
import { palletCfLendingPoolsGeneralLendingWhitelistWhitelistUpdate } from '../common';

export const lendingPoolsWhitelistUpdated = z.object({
  update: palletCfLendingPoolsGeneralLendingWhitelistWhitelistUpdate,
});
