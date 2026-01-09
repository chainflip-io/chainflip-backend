import { z } from 'zod';
import { hexString } from '../common';

export const assethubBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });
