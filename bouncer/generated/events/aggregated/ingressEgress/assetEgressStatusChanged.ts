import { arbitrumIngressEgressAssetEgressStatusChangedEvent } from '../../arbitrumIngressEgress/assetEgressStatusChanged';
import { assethubIngressEgressAssetEgressStatusChangedEvent } from '../../assethubIngressEgress/assetEgressStatusChanged';
import { bitcoinIngressEgressAssetEgressStatusChangedEvent } from '../../bitcoinIngressEgress/assetEgressStatusChanged';
import { bscIngressEgressAssetEgressStatusChangedEvent } from '../../bscIngressEgress/assetEgressStatusChanged';
import { ethereumIngressEgressAssetEgressStatusChangedEvent } from '../../ethereumIngressEgress/assetEgressStatusChanged';
import { polkadotIngressEgressAssetEgressStatusChangedEvent } from '../../polkadotIngressEgress/assetEgressStatusChanged';
import { solanaIngressEgressAssetEgressStatusChangedEvent } from '../../solanaIngressEgress/assetEgressStatusChanged';
import { tronIngressEgressAssetEgressStatusChangedEvent } from '../../tronIngressEgress/assetEgressStatusChanged';

export const ingressEgressAssetEgressStatusChangedEvent = {
  Arbitrum: arbitrumIngressEgressAssetEgressStatusChangedEvent,
  Assethub: assethubIngressEgressAssetEgressStatusChangedEvent,
  Bitcoin: bitcoinIngressEgressAssetEgressStatusChangedEvent,
  Bsc: bscIngressEgressAssetEgressStatusChangedEvent,
  Ethereum: ethereumIngressEgressAssetEgressStatusChangedEvent,
  Polkadot: polkadotIngressEgressAssetEgressStatusChangedEvent,
  Solana: solanaIngressEgressAssetEgressStatusChangedEvent,
  Tron: tronIngressEgressAssetEgressStatusChangedEvent,
} as const;
