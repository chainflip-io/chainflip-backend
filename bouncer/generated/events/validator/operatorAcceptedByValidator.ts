import { z } from 'zod';
import { accountId } from '../common';

export const validatorOperatorAcceptedByValidator = z.object({
  validator: accountId,
  operator: accountId,
});
