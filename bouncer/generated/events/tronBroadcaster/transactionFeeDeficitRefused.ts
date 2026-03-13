import { z } from 'zod';
import { hexString } from '../common';

export const tronBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });
