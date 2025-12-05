import { z } from 'zod';
import { accountId, numberOrHex } from '../common';

export const fundingRedemptionSettled = z.tuple([accountId, numberOrHex]);
