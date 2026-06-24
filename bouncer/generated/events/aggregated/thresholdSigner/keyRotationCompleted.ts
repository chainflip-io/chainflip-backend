import { evmThresholdSignerKeyRotationCompletedEvent } from '../../evmThresholdSigner/keyRotationCompleted';
import { polkadotThresholdSignerKeyRotationCompletedEvent } from '../../polkadotThresholdSigner/keyRotationCompleted';
import { bitcoinThresholdSignerKeyRotationCompletedEvent } from '../../bitcoinThresholdSigner/keyRotationCompleted';
import { solanaThresholdSignerKeyRotationCompletedEvent } from '../../solanaThresholdSigner/keyRotationCompleted';

export const thresholdSignerKeyRotationCompletedEvent = {
  Arbitrum: evmThresholdSignerKeyRotationCompletedEvent,
  Assethub: polkadotThresholdSignerKeyRotationCompletedEvent,
  Bitcoin: bitcoinThresholdSignerKeyRotationCompletedEvent,
  Ethereum: evmThresholdSignerKeyRotationCompletedEvent,
  Polkadot: polkadotThresholdSignerKeyRotationCompletedEvent,
  Solana: solanaThresholdSignerKeyRotationCompletedEvent,
} as const;
