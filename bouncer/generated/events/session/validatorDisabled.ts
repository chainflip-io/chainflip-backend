import { z } from 'zod';
import { accountId } from '../common';

export const sessionValidatorDisabled = z.object({ validator: accountId });
