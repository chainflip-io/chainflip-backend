import { arbitrumChainTrackingFeeMultiplierUpdatedEvent } from '../../arbitrumChainTracking/feeMultiplierUpdated';
import { assethubChainTrackingFeeMultiplierUpdatedEvent } from '../../assethubChainTracking/feeMultiplierUpdated';
import { bitcoinChainTrackingFeeMultiplierUpdatedEvent } from '../../bitcoinChainTracking/feeMultiplierUpdated';
import { bscChainTrackingFeeMultiplierUpdatedEvent } from '../../bscChainTracking/feeMultiplierUpdated';
import { ethereumChainTrackingFeeMultiplierUpdatedEvent } from '../../ethereumChainTracking/feeMultiplierUpdated';
import { polkadotChainTrackingFeeMultiplierUpdatedEvent } from '../../polkadotChainTracking/feeMultiplierUpdated';
import { solanaChainTrackingFeeMultiplierUpdatedEvent } from '../../solanaChainTracking/feeMultiplierUpdated';
import { tronChainTrackingFeeMultiplierUpdatedEvent } from '../../tronChainTracking/feeMultiplierUpdated';

export const chainTrackingFeeMultiplierUpdatedEvent = {
  Arbitrum: arbitrumChainTrackingFeeMultiplierUpdatedEvent,
  Assethub: assethubChainTrackingFeeMultiplierUpdatedEvent,
  Bitcoin: bitcoinChainTrackingFeeMultiplierUpdatedEvent,
  Bsc: bscChainTrackingFeeMultiplierUpdatedEvent,
  Ethereum: ethereumChainTrackingFeeMultiplierUpdatedEvent,
  Polkadot: polkadotChainTrackingFeeMultiplierUpdatedEvent,
  Solana: solanaChainTrackingFeeMultiplierUpdatedEvent,
  Tron: tronChainTrackingFeeMultiplierUpdatedEvent,
} as const;
