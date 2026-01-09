import { z } from 'zod';
import { accountId } from '../common';

export const systemNewAccount = z.object({ account: accountId });
