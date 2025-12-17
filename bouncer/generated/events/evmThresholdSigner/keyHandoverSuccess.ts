import { z } from 'zod';
import { numberOrHex } from '../common';

export const evmThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });
