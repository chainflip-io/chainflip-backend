import { z } from 'zod';
import { hexString } from '../common';

export const arbitrumBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });
