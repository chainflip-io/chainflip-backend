import { z } from 'zod';
import {
  numberOrHex,
  palletCfTronIngressEgressDepositFailedDetailsTron,
  palletCfTronIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfTronIngressEgressDepositFailedReason,
  details: palletCfTronIngressEgressDepositFailedDetailsTron,
});

export const tronIngressEgressDepositFailedEvent = defineEvent(
  'TronIngressEgress.DepositFailed',
  tronIngressEgressDepositFailed,
);
