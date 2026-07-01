import { arbitrumChainTrackingChainStateUpdatedEvent } from '../../arbitrumChainTracking/chainStateUpdated';
import { assethubChainTrackingChainStateUpdatedEvent } from '../../assethubChainTracking/chainStateUpdated';
import { bitcoinChainTrackingChainStateUpdatedEvent } from '../../bitcoinChainTracking/chainStateUpdated';
import { bscChainTrackingChainStateUpdatedEvent } from '../../bscChainTracking/chainStateUpdated';
import { ethereumChainTrackingChainStateUpdatedEvent } from '../../ethereumChainTracking/chainStateUpdated';
import { polkadotChainTrackingChainStateUpdatedEvent } from '../../polkadotChainTracking/chainStateUpdated';
import { solanaChainTrackingChainStateUpdatedEvent } from '../../solanaChainTracking/chainStateUpdated';
import { tronChainTrackingChainStateUpdatedEvent } from '../../tronChainTracking/chainStateUpdated';

export const chainTrackingChainStateUpdatedEvent = {
  Arbitrum: arbitrumChainTrackingChainStateUpdatedEvent,
  Assethub: assethubChainTrackingChainStateUpdatedEvent,
  Bitcoin: bitcoinChainTrackingChainStateUpdatedEvent,
  Bsc: bscChainTrackingChainStateUpdatedEvent,
  Ethereum: ethereumChainTrackingChainStateUpdatedEvent,
  Polkadot: polkadotChainTrackingChainStateUpdatedEvent,
  Solana: solanaChainTrackingChainStateUpdatedEvent,
  Tron: tronChainTrackingChainStateUpdatedEvent,
} as const;
