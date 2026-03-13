import { z } from 'zod';
import { hexString } from '../common';

export const bscBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });
