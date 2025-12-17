import { z } from 'zod';
import { hexString } from '../common';

export const solanaThresholdSignerKeyHandoverVerificationSuccess = z.object({ aggKey: hexString });
