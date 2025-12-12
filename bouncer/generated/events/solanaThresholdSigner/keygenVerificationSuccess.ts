import { z } from 'zod';
import { hexString } from '../common';

export const solanaThresholdSignerKeygenVerificationSuccess = z.object({ aggKey: hexString });
