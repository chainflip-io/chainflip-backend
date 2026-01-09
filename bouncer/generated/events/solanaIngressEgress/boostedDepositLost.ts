import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
