import { z } from 'zod';
import { accountId, hexString } from '../common';

export const fundingRedemptionExpired = z.object({ accountId, txHash: hexString });
