import { z } from 'zod';
import {
  palletCfPolkadotIngressEgressDepositFailedDetailsPolkadot,
  palletCfPolkadotIngressEgressDepositFailedReason,
} from '../common';

export const polkadotIngressEgressDepositFailed = z.object({
  blockHeight: z.number(),
  reason: palletCfPolkadotIngressEgressDepositFailedReason,
  details: palletCfPolkadotIngressEgressDepositFailedDetailsPolkadot,
});
