import { z } from 'zod';
import {
  numberOrHex,
  palletCfBitcoinIngressEgressDepositFailedDetailsBitcoin,
  palletCfBitcoinIngressEgressDepositFailedReason,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressDepositFailed = z.object({
  blockHeight: numberOrHex,
  reason: palletCfBitcoinIngressEgressDepositFailedReason,
  details: palletCfBitcoinIngressEgressDepositFailedDetailsBitcoin,
});

export const bitcoinIngressEgressDepositFailedEvent = defineEvent(
  'BitcoinIngressEgress.DepositFailed',
  bitcoinIngressEgressDepositFailed,
);
