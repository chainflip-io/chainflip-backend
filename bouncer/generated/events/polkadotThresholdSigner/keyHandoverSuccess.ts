import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotThresholdSignerKeyHandoverSuccess = z.object({ ceremonyId: numberOrHex });
