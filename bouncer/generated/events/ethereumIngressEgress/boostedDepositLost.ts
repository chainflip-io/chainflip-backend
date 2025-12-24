import { z } from 'zod';
import { numberOrHex } from '../common';

export const ethereumIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
