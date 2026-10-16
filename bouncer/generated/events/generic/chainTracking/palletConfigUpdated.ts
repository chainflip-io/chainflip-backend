import { arbitrumChainTrackingPalletConfigUpdatedEvent } from '../../arbitrumChainTracking/palletConfigUpdated';
import { assethubChainTrackingPalletConfigUpdatedEvent } from '../../assethubChainTracking/palletConfigUpdated';
import { bitcoinChainTrackingPalletConfigUpdatedEvent } from '../../bitcoinChainTracking/palletConfigUpdated';
import { bscChainTrackingPalletConfigUpdatedEvent } from '../../bscChainTracking/palletConfigUpdated';
import { ethereumChainTrackingPalletConfigUpdatedEvent } from '../../ethereumChainTracking/palletConfigUpdated';
import { polkadotChainTrackingPalletConfigUpdatedEvent } from '../../polkadotChainTracking/palletConfigUpdated';
import { solanaChainTrackingPalletConfigUpdatedEvent } from '../../solanaChainTracking/palletConfigUpdated';
import { tronChainTrackingPalletConfigUpdatedEvent } from '../../tronChainTracking/palletConfigUpdated';

export const chainTrackingPalletConfigUpdatedEvent = {
  Arbitrum: arbitrumChainTrackingPalletConfigUpdatedEvent,
  Assethub: assethubChainTrackingPalletConfigUpdatedEvent,
  Bitcoin: bitcoinChainTrackingPalletConfigUpdatedEvent,
  Bsc: bscChainTrackingPalletConfigUpdatedEvent,
  Ethereum: ethereumChainTrackingPalletConfigUpdatedEvent,
  Polkadot: polkadotChainTrackingPalletConfigUpdatedEvent,
  Solana: solanaChainTrackingPalletConfigUpdatedEvent,
  Tron: tronChainTrackingPalletConfigUpdatedEvent,
} as const;
