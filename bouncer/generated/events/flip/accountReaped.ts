import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const flipAccountReaped = z.object({ who: accountId, dustBurned: numberOrHex });
