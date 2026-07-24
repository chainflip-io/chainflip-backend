import { bitcoinThresholdSignerPalletConfigUpdatedEvent } from '../../bitcoinThresholdSigner/palletConfigUpdated';
import { evmThresholdSignerPalletConfigUpdatedEvent } from '../../evmThresholdSigner/palletConfigUpdated';
import { polkadotThresholdSignerPalletConfigUpdatedEvent } from '../../polkadotThresholdSigner/palletConfigUpdated';
import { solanaThresholdSignerPalletConfigUpdatedEvent } from '../../solanaThresholdSigner/palletConfigUpdated';

export const thresholdSignerPalletConfigUpdatedEvent = {
  Bitcoin: bitcoinThresholdSignerPalletConfigUpdatedEvent,
  Evm: evmThresholdSignerPalletConfigUpdatedEvent,
  Polkadot: polkadotThresholdSignerPalletConfigUpdatedEvent,
  Solana: solanaThresholdSignerPalletConfigUpdatedEvent,
} as const;
