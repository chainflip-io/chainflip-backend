import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });
