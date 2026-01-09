import { z } from 'zod';
import { numberOrHex } from '../common';

export const arbitrumIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
