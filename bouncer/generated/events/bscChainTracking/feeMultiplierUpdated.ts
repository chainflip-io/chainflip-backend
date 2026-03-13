import { z } from 'zod';
import { numberOrHex } from '../common';

export const bscChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });
