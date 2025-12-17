import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const flipBondUpdated = z.object({ accountId, newBond: numberOrHex });
