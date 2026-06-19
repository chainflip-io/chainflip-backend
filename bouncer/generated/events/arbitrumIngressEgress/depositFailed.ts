import { z } from 'zod';
import {
  numberOrHex,
  palletCfArbitrumIngressEgressDepositFailedDetailsArbitrum,
  palletCfArbitrumIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfArbitrumIngressEgressDepositFailedReason,
  details: palletCfArbitrumIngressEgressDepositFailedDetailsArbitrum,
});

export const arbitrumIngressEgressDepositFailedEvent = defineEvent(
  'ArbitrumIngressEgress.DepositFailed',
  arbitrumIngressEgressDepositFailed,
);
