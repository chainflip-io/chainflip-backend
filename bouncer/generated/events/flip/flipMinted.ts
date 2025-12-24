import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const flipFlipMinted = z.object({ to: accountId, amount: numberOrHex });
