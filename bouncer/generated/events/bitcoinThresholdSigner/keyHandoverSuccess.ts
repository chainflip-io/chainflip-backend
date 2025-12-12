import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });
