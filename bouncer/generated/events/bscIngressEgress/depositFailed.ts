import { z } from 'zod';
import {
  numberOrHex,
  palletCfBscIngressEgressDepositFailedDetailsBsc,
  palletCfBscIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfBscIngressEgressDepositFailedReason,
  details: palletCfBscIngressEgressDepositFailedDetailsBsc,
});

export const bscIngressEgressDepositFailedEvent = defineEvent(
  'BscIngressEgress.DepositFailed',
  bscIngressEgressDepositFailed,
);
