import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const tronIngressEgressBoostedDepositLostEvent = defineEvent(
  'TronIngressEgress.BoostedDepositLost',
  tronIngressEgressBoostedDepositLost,
);
