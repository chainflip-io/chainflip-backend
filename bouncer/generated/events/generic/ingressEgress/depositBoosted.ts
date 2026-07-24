import { arbitrumIngressEgressDepositBoostedEvent } from '../../arbitrumIngressEgress/depositBoosted';
import { assethubIngressEgressDepositBoostedEvent } from '../../assethubIngressEgress/depositBoosted';
import { bitcoinIngressEgressDepositBoostedEvent } from '../../bitcoinIngressEgress/depositBoosted';
import { bscIngressEgressDepositBoostedEvent } from '../../bscIngressEgress/depositBoosted';
import { ethereumIngressEgressDepositBoostedEvent } from '../../ethereumIngressEgress/depositBoosted';
import { polkadotIngressEgressDepositBoostedEvent } from '../../polkadotIngressEgress/depositBoosted';
import { solanaIngressEgressDepositBoostedEvent } from '../../solanaIngressEgress/depositBoosted';
import { tronIngressEgressDepositBoostedEvent } from '../../tronIngressEgress/depositBoosted';

export const ingressEgressDepositBoostedEvent = {
  Arbitrum: arbitrumIngressEgressDepositBoostedEvent,
  Assethub: assethubIngressEgressDepositBoostedEvent,
  Bitcoin: bitcoinIngressEgressDepositBoostedEvent,
  Bsc: bscIngressEgressDepositBoostedEvent,
  Ethereum: ethereumIngressEgressDepositBoostedEvent,
  Polkadot: polkadotIngressEgressDepositBoostedEvent,
  Solana: solanaIngressEgressDepositBoostedEvent,
  Tron: tronIngressEgressDepositBoostedEvent,
} as const;
