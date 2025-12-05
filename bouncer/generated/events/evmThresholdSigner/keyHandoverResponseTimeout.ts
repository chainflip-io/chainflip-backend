import { z } from 'zod';
import { numberOrHex } from '../common';

export const evmThresholdSignerKeyHandoverResponseTimeout = z.object({ ceremonyId: numberOrHex });
