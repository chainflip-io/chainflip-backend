import { z } from 'zod';
import { accountId, cfPrimitivesSemVer } from '../common';

export const validatorCFEVersionUpdated = z.object({
  accountId,
  oldVersion: cfPrimitivesSemVer,
  newVersion: cfPrimitivesSemVer,
});
