import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const ethereumIngressEgressBoostedDepositLostEvent = defineEvent(
  'EthereumIngressEgress.BoostedDepositLost',
  ethereumIngressEgressBoostedDepositLost,
);
