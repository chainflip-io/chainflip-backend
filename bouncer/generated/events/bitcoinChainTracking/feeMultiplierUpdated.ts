import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinChainTrackingFeeMultiplierUpdated = z.object({ newFeeMultiplier: numberOrHex });
