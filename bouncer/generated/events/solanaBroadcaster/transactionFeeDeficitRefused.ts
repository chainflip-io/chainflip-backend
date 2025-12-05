import { z } from 'zod';
import { hexString } from '../common';

export const solanaBroadcasterTransactionFeeDeficitRefused = z.object({ beneficiary: hexString });
