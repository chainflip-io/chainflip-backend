import { z } from 'zod';
import { numberOrHex } from '../common';

export const assethubIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
