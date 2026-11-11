import { arbitrumIngressEgressFailedDepositRecoveryFailedEvent } from '../../arbitrumIngressEgress/failedDepositRecoveryFailed';
import { assethubIngressEgressFailedDepositRecoveryFailedEvent } from '../../assethubIngressEgress/failedDepositRecoveryFailed';
import { bitcoinIngressEgressFailedDepositRecoveryFailedEvent } from '../../bitcoinIngressEgress/failedDepositRecoveryFailed';
import { bscIngressEgressFailedDepositRecoveryFailedEvent } from '../../bscIngressEgress/failedDepositRecoveryFailed';
import { ethereumIngressEgressFailedDepositRecoveryFailedEvent } from '../../ethereumIngressEgress/failedDepositRecoveryFailed';
import { polkadotIngressEgressFailedDepositRecoveryFailedEvent } from '../../polkadotIngressEgress/failedDepositRecoveryFailed';
import { solanaIngressEgressFailedDepositRecoveryFailedEvent } from '../../solanaIngressEgress/failedDepositRecoveryFailed';
import { tronIngressEgressFailedDepositRecoveryFailedEvent } from '../../tronIngressEgress/failedDepositRecoveryFailed';

export const ingressEgressFailedDepositRecoveryFailedEvent = {
  Arbitrum: arbitrumIngressEgressFailedDepositRecoveryFailedEvent,
  Assethub: assethubIngressEgressFailedDepositRecoveryFailedEvent,
  Bitcoin: bitcoinIngressEgressFailedDepositRecoveryFailedEvent,
  Bsc: bscIngressEgressFailedDepositRecoveryFailedEvent,
  Ethereum: ethereumIngressEgressFailedDepositRecoveryFailedEvent,
  Polkadot: polkadotIngressEgressFailedDepositRecoveryFailedEvent,
  Solana: solanaIngressEgressFailedDepositRecoveryFailedEvent,
  Tron: tronIngressEgressFailedDepositRecoveryFailedEvent,
} as const;
