import { z } from 'zod';
import {
  numberOrHex,
  palletCfEthereumIngressEgressDepositFailedDetailsEthereum,
  palletCfEthereumIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfEthereumIngressEgressDepositFailedReason,
  details: palletCfEthereumIngressEgressDepositFailedDetailsEthereum,
});

export const ethereumIngressEgressDepositFailedEvent = defineEvent(
  'EthereumIngressEgress.DepositFailed',
  ethereumIngressEgressDepositFailed,
);
