import { z } from 'zod';
import { numberOrHex } from '../common';

export const tronIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
