import { z } from 'zod';
import { accountId } from '../common';

export const validatorValidatorRemovedFromOperator = z.object({
  validator: accountId,
  operator: accountId,
});
