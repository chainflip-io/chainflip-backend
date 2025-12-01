import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });
