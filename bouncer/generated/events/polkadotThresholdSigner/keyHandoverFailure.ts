import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotThresholdSignerKeyHandoverFailure = z.object({ ceremonyId: numberOrHex });
