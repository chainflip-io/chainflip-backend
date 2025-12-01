import { z } from 'zod';
import { hexString } from '../common';

export const ethereumBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });
