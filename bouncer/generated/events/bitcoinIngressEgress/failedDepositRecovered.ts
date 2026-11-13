import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfBitcoinIngressEgressFailedVaultDepositsFailedVaultDepositInfoBitcoin,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfBitcoinIngressEgressFailedVaultDepositsFailedVaultDepositInfoBitcoin,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const bitcoinIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'BitcoinIngressEgress.FailedDepositRecovered',
  bitcoinIngressEgressFailedDepositRecovered,
);
