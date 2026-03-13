import { z } from 'zod';
import { numberOrHex } from '../common';

export const tronChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });
