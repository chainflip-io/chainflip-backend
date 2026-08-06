import { arbitrumIngressEgressPalletConfigUpdatedEvent } from '../../arbitrumIngressEgress/palletConfigUpdated';
import { assethubIngressEgressPalletConfigUpdatedEvent } from '../../assethubIngressEgress/palletConfigUpdated';
import { bitcoinIngressEgressPalletConfigUpdatedEvent } from '../../bitcoinIngressEgress/palletConfigUpdated';
import { bscIngressEgressPalletConfigUpdatedEvent } from '../../bscIngressEgress/palletConfigUpdated';
import { ethereumIngressEgressPalletConfigUpdatedEvent } from '../../ethereumIngressEgress/palletConfigUpdated';
import { polkadotIngressEgressPalletConfigUpdatedEvent } from '../../polkadotIngressEgress/palletConfigUpdated';
import { solanaIngressEgressPalletConfigUpdatedEvent } from '../../solanaIngressEgress/palletConfigUpdated';
import { tronIngressEgressPalletConfigUpdatedEvent } from '../../tronIngressEgress/palletConfigUpdated';

export const ingressEgressPalletConfigUpdatedEvent = {
  Arbitrum: arbitrumIngressEgressPalletConfigUpdatedEvent,
  Assethub: assethubIngressEgressPalletConfigUpdatedEvent,
  Bitcoin: bitcoinIngressEgressPalletConfigUpdatedEvent,
  Bsc: bscIngressEgressPalletConfigUpdatedEvent,
  Ethereum: ethereumIngressEgressPalletConfigUpdatedEvent,
  Polkadot: polkadotIngressEgressPalletConfigUpdatedEvent,
  Solana: solanaIngressEgressPalletConfigUpdatedEvent,
  Tron: tronIngressEgressPalletConfigUpdatedEvent,
} as const;
