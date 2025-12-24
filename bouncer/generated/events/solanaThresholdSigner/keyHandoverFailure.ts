import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });
