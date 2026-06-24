import { arbitrumIngressEgressInsufficientBoostLiquidityEvent } from '../../arbitrumIngressEgress/insufficientBoostLiquidity';
import { assethubIngressEgressInsufficientBoostLiquidityEvent } from '../../assethubIngressEgress/insufficientBoostLiquidity';
import { bitcoinIngressEgressInsufficientBoostLiquidityEvent } from '../../bitcoinIngressEgress/insufficientBoostLiquidity';
import { bscIngressEgressInsufficientBoostLiquidityEvent } from '../../bscIngressEgress/insufficientBoostLiquidity';
import { ethereumIngressEgressInsufficientBoostLiquidityEvent } from '../../ethereumIngressEgress/insufficientBoostLiquidity';
import { polkadotIngressEgressInsufficientBoostLiquidityEvent } from '../../polkadotIngressEgress/insufficientBoostLiquidity';
import { solanaIngressEgressInsufficientBoostLiquidityEvent } from '../../solanaIngressEgress/insufficientBoostLiquidity';
import { tronIngressEgressInsufficientBoostLiquidityEvent } from '../../tronIngressEgress/insufficientBoostLiquidity';

export const ingressEgressInsufficientBoostLiquidityEvent = {
  Arbitrum: arbitrumIngressEgressInsufficientBoostLiquidityEvent,
  Assethub: assethubIngressEgressInsufficientBoostLiquidityEvent,
  Bitcoin: bitcoinIngressEgressInsufficientBoostLiquidityEvent,
  Bsc: bscIngressEgressInsufficientBoostLiquidityEvent,
  Ethereum: ethereumIngressEgressInsufficientBoostLiquidityEvent,
  Polkadot: polkadotIngressEgressInsufficientBoostLiquidityEvent,
  Solana: solanaIngressEgressInsufficientBoostLiquidityEvent,
  Tron: tronIngressEgressInsufficientBoostLiquidityEvent,
} as const;
