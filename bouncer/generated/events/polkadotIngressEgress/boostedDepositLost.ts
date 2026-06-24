import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressBoostedDepositLost = z.object({
  prewitnessedDepositId: numberOrHex,
  amount: numberOrHex,
});

export const polkadotIngressEgressBoostedDepositLostEvent = defineEvent(
  'PolkadotIngressEgress.BoostedDepositLost',
  polkadotIngressEgressBoostedDepositLost,
);
