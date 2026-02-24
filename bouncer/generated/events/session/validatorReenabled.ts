import { z } from 'zod';
import { accountId } from '../common';

export const sessionValidatorReenabled = z.object({ validator: accountId });
