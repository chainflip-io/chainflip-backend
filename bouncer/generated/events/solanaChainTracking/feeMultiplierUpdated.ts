import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });
