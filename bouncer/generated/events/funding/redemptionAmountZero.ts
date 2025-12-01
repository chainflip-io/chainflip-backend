import { z } from 'zod';
import { accountId } from '../common';

export const fundingRedemptionAmountZero = z.object({ accountId });
