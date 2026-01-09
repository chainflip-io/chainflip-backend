import { z } from 'zod';
import { numberOrHex } from '../common';

export const evmThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });
