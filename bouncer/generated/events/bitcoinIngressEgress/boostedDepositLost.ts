import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const bitcoinIngressEgressBoostedDepositLostEvent = defineEvent(
  'BitcoinIngressEgress.BoostedDepositLost',
  bitcoinIngressEgressBoostedDepositLost,
);
