import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const bscIngressEgressBoostedDepositLostEvent = defineEvent(
  'BscIngressEgress.BoostedDepositLost',
  bscIngressEgressBoostedDepositLost,
);
