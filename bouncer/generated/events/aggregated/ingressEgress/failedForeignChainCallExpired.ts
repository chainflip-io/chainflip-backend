import { arbitrumIngressEgressFailedForeignChainCallExpiredEvent } from '../../arbitrumIngressEgress/failedForeignChainCallExpired';
import { assethubIngressEgressFailedForeignChainCallExpiredEvent } from '../../assethubIngressEgress/failedForeignChainCallExpired';
import { bitcoinIngressEgressFailedForeignChainCallExpiredEvent } from '../../bitcoinIngressEgress/failedForeignChainCallExpired';
import { bscIngressEgressFailedForeignChainCallExpiredEvent } from '../../bscIngressEgress/failedForeignChainCallExpired';
import { ethereumIngressEgressFailedForeignChainCallExpiredEvent } from '../../ethereumIngressEgress/failedForeignChainCallExpired';
import { polkadotIngressEgressFailedForeignChainCallExpiredEvent } from '../../polkadotIngressEgress/failedForeignChainCallExpired';
import { solanaIngressEgressFailedForeignChainCallExpiredEvent } from '../../solanaIngressEgress/failedForeignChainCallExpired';
import { tronIngressEgressFailedForeignChainCallExpiredEvent } from '../../tronIngressEgress/failedForeignChainCallExpired';

export const ingressEgressFailedForeignChainCallExpiredEvent = {
  Arbitrum: arbitrumIngressEgressFailedForeignChainCallExpiredEvent,
  Assethub: assethubIngressEgressFailedForeignChainCallExpiredEvent,
  Bitcoin: bitcoinIngressEgressFailedForeignChainCallExpiredEvent,
  Bsc: bscIngressEgressFailedForeignChainCallExpiredEvent,
  Ethereum: ethereumIngressEgressFailedForeignChainCallExpiredEvent,
  Polkadot: polkadotIngressEgressFailedForeignChainCallExpiredEvent,
  Solana: solanaIngressEgressFailedForeignChainCallExpiredEvent,
  Tron: tronIngressEgressFailedForeignChainCallExpiredEvent,
} as const;
