import { z } from 'zod';
import { accountId } from '../common';

export const fundingRedemptionExpired = z.object({ accountId });
