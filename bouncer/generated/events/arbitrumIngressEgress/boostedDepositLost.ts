import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const arbitrumIngressEgressBoostedDepositLostEvent = defineEvent(
  'ArbitrumIngressEgress.BoostedDepositLost',
  arbitrumIngressEgressBoostedDepositLost,
);
