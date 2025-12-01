import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});
