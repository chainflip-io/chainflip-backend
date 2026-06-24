import { z } from 'zod';
import {
  palletCfPolkadotIngressEgressDepositFailedDetailsPolkadot,
  palletCfPolkadotIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressDepositFailed = z.object({
  blockHeight: z.number(),
  reason: palletCfPolkadotIngressEgressDepositFailedReason,
  details: palletCfPolkadotIngressEgressDepositFailedDetailsPolkadot,
});

export const polkadotIngressEgressDepositFailedEvent = defineEvent(
  'PolkadotIngressEgress.DepositFailed',
  polkadotIngressEgressDepositFailed,
);
