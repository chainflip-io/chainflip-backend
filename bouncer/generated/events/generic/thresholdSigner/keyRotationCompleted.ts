import { bitcoinThresholdSignerKeyRotationCompletedEvent } from '../../bitcoinThresholdSigner/keyRotationCompleted';
import { evmThresholdSignerKeyRotationCompletedEvent } from '../../evmThresholdSigner/keyRotationCompleted';
import { polkadotThresholdSignerKeyRotationCompletedEvent } from '../../polkadotThresholdSigner/keyRotationCompleted';
import { solanaThresholdSignerKeyRotationCompletedEvent } from '../../solanaThresholdSigner/keyRotationCompleted';

export const thresholdSignerKeyRotationCompletedEvent = {
  Bitcoin: bitcoinThresholdSignerKeyRotationCompletedEvent,
  Evm: evmThresholdSignerKeyRotationCompletedEvent,
  Polkadot: polkadotThresholdSignerKeyRotationCompletedEvent,
  Solana: solanaThresholdSignerKeyRotationCompletedEvent,
} as const;
