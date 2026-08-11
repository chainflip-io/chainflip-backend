import { arbitrumIngressEgressFailedForeignChainCallResignedEvent } from '../../arbitrumIngressEgress/failedForeignChainCallResigned';
import { assethubIngressEgressFailedForeignChainCallResignedEvent } from '../../assethubIngressEgress/failedForeignChainCallResigned';
import { bitcoinIngressEgressFailedForeignChainCallResignedEvent } from '../../bitcoinIngressEgress/failedForeignChainCallResigned';
import { bscIngressEgressFailedForeignChainCallResignedEvent } from '../../bscIngressEgress/failedForeignChainCallResigned';
import { ethereumIngressEgressFailedForeignChainCallResignedEvent } from '../../ethereumIngressEgress/failedForeignChainCallResigned';
import { polkadotIngressEgressFailedForeignChainCallResignedEvent } from '../../polkadotIngressEgress/failedForeignChainCallResigned';
import { solanaIngressEgressFailedForeignChainCallResignedEvent } from '../../solanaIngressEgress/failedForeignChainCallResigned';
import { tronIngressEgressFailedForeignChainCallResignedEvent } from '../../tronIngressEgress/failedForeignChainCallResigned';

export const ingressEgressFailedForeignChainCallResignedEvent = {
  Arbitrum: arbitrumIngressEgressFailedForeignChainCallResignedEvent,
  Assethub: assethubIngressEgressFailedForeignChainCallResignedEvent,
  Bitcoin: bitcoinIngressEgressFailedForeignChainCallResignedEvent,
  Bsc: bscIngressEgressFailedForeignChainCallResignedEvent,
  Ethereum: ethereumIngressEgressFailedForeignChainCallResignedEvent,
  Polkadot: polkadotIngressEgressFailedForeignChainCallResignedEvent,
  Solana: solanaIngressEgressFailedForeignChainCallResignedEvent,
  Tron: tronIngressEgressFailedForeignChainCallResignedEvent,
} as const;
