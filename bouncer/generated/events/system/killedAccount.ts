import { z } from 'zod';
import { accountId } from '../common';

export const systemKilledAccount = z.object({ account: accountId });
