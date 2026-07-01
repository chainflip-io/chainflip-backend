import { arbitrumIngressEgressFailedToBuildAllBatchCallEvent } from '../../arbitrumIngressEgress/failedToBuildAllBatchCall';
import { assethubIngressEgressFailedToBuildAllBatchCallEvent } from '../../assethubIngressEgress/failedToBuildAllBatchCall';
import { bitcoinIngressEgressFailedToBuildAllBatchCallEvent } from '../../bitcoinIngressEgress/failedToBuildAllBatchCall';
import { bscIngressEgressFailedToBuildAllBatchCallEvent } from '../../bscIngressEgress/failedToBuildAllBatchCall';
import { ethereumIngressEgressFailedToBuildAllBatchCallEvent } from '../../ethereumIngressEgress/failedToBuildAllBatchCall';
import { polkadotIngressEgressFailedToBuildAllBatchCallEvent } from '../../polkadotIngressEgress/failedToBuildAllBatchCall';
import { solanaIngressEgressFailedToBuildAllBatchCallEvent } from '../../solanaIngressEgress/failedToBuildAllBatchCall';
import { tronIngressEgressFailedToBuildAllBatchCallEvent } from '../../tronIngressEgress/failedToBuildAllBatchCall';

export const ingressEgressFailedToBuildAllBatchCallEvent = {
  Arbitrum: arbitrumIngressEgressFailedToBuildAllBatchCallEvent,
  Assethub: assethubIngressEgressFailedToBuildAllBatchCallEvent,
  Bitcoin: bitcoinIngressEgressFailedToBuildAllBatchCallEvent,
  Bsc: bscIngressEgressFailedToBuildAllBatchCallEvent,
  Ethereum: ethereumIngressEgressFailedToBuildAllBatchCallEvent,
  Polkadot: polkadotIngressEgressFailedToBuildAllBatchCallEvent,
  Solana: solanaIngressEgressFailedToBuildAllBatchCallEvent,
  Tron: tronIngressEgressFailedToBuildAllBatchCallEvent,
} as const;
