import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const solanaIngressEgressBoostedDepositLostEvent = defineEvent(
  'SolanaIngressEgress.BoostedDepositLost',
  solanaIngressEgressBoostedDepositLost,
);
