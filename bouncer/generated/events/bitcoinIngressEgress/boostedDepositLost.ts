import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
