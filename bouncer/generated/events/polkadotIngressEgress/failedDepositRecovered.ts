import { z } from 'zod';
import {
  accountId,
  cfTraitsFailedDepositRecoveryOutcome,
  hexString,
  palletCfPolkadotIngressEgressFailedVaultDepositsFailedVaultDepositInfoPolkadot,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressFailedDepositRecovered = z.object({
  queueId: hexString.nullish(),
  deposit: palletCfPolkadotIngressEgressFailedVaultDepositsFailedVaultDepositInfoPolkadot,
  brokerId: accountId,
  recoveryAccount: accountId,
  outcome: cfTraitsFailedDepositRecoveryOutcome,
});

export const polkadotIngressEgressFailedDepositRecoveredEvent = defineEvent(
  'PolkadotIngressEgress.FailedDepositRecovered',
  polkadotIngressEgressFailedDepositRecovered,
);
