import { arbitrumIngressEgressUtxoConsolidationEvent } from '../../arbitrumIngressEgress/utxoConsolidation';
import { assethubIngressEgressUtxoConsolidationEvent } from '../../assethubIngressEgress/utxoConsolidation';
import { bitcoinIngressEgressUtxoConsolidationEvent } from '../../bitcoinIngressEgress/utxoConsolidation';
import { bscIngressEgressUtxoConsolidationEvent } from '../../bscIngressEgress/utxoConsolidation';
import { ethereumIngressEgressUtxoConsolidationEvent } from '../../ethereumIngressEgress/utxoConsolidation';
import { polkadotIngressEgressUtxoConsolidationEvent } from '../../polkadotIngressEgress/utxoConsolidation';
import { solanaIngressEgressUtxoConsolidationEvent } from '../../solanaIngressEgress/utxoConsolidation';
import { tronIngressEgressUtxoConsolidationEvent } from '../../tronIngressEgress/utxoConsolidation';

export const ingressEgressUtxoConsolidationEvent = {
  Arbitrum: arbitrumIngressEgressUtxoConsolidationEvent,
  Assethub: assethubIngressEgressUtxoConsolidationEvent,
  Bitcoin: bitcoinIngressEgressUtxoConsolidationEvent,
  Bsc: bscIngressEgressUtxoConsolidationEvent,
  Ethereum: ethereumIngressEgressUtxoConsolidationEvent,
  Polkadot: polkadotIngressEgressUtxoConsolidationEvent,
  Solana: solanaIngressEgressUtxoConsolidationEvent,
  Tron: tronIngressEgressUtxoConsolidationEvent,
} as const;
