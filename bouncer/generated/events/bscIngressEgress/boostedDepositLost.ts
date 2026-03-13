import { z } from 'zod';
import { numberOrHex } from '../common';

export const bscIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
