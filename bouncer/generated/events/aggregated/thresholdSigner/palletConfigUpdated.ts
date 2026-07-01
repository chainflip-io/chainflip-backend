import { evmThresholdSignerPalletConfigUpdatedEvent } from '../../evmThresholdSigner/palletConfigUpdated';
import { polkadotThresholdSignerPalletConfigUpdatedEvent } from '../../polkadotThresholdSigner/palletConfigUpdated';
import { bitcoinThresholdSignerPalletConfigUpdatedEvent } from '../../bitcoinThresholdSigner/palletConfigUpdated';
import { solanaThresholdSignerPalletConfigUpdatedEvent } from '../../solanaThresholdSigner/palletConfigUpdated';

export const thresholdSignerPalletConfigUpdatedEvent = {
  Arbitrum: evmThresholdSignerPalletConfigUpdatedEvent,
  Assethub: polkadotThresholdSignerPalletConfigUpdatedEvent,
  Bitcoin: bitcoinThresholdSignerPalletConfigUpdatedEvent,
  Ethereum: evmThresholdSignerPalletConfigUpdatedEvent,
  Polkadot: polkadotThresholdSignerPalletConfigUpdatedEvent,
  Solana: solanaThresholdSignerPalletConfigUpdatedEvent,
} as const;
