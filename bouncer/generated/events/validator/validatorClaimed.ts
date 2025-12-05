import { z } from 'zod';
import { accountId } from '../common';

export const validatorValidatorClaimed = z.object({ validator: accountId, operator: accountId });
