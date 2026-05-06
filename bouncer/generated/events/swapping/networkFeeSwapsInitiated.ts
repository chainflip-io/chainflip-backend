import { z } from 'zod';
import { numberOrHex } from '../common';

export const swappingNetworkFeeSwapsInitiated = z.object({ swapRequestIds: z.array(numberOrHex) });
