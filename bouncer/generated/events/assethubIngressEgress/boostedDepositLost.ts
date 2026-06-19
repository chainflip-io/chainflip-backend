import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const assethubIngressEgressBoostedDepositLostEvent = defineEvent(
  'AssethubIngressEgress.BoostedDepositLost',
  assethubIngressEgressBoostedDepositLost,
);
