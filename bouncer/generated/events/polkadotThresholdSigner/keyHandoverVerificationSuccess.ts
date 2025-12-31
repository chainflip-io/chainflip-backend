import { z } from 'zod';
import { hexString } from '../common';

export const polkadotThresholdSignerKeyHandoverVerificationSuccess = z.object({
  aggKey: hexString,
});
