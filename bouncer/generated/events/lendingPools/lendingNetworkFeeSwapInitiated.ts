import { z } from 'zod';
import { numberOrHex } from '../common';

export const lendingPoolsLendingNetworkFeeSwapInitiated = z.object({ swapRequestId: numberOrHex });
