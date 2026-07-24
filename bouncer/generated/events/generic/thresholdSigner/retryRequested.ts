import { bitcoinThresholdSignerRetryRequestedEvent } from '../../bitcoinThresholdSigner/retryRequested';
import { evmThresholdSignerRetryRequestedEvent } from '../../evmThresholdSigner/retryRequested';
import { polkadotThresholdSignerRetryRequestedEvent } from '../../polkadotThresholdSigner/retryRequested';
import { solanaThresholdSignerRetryRequestedEvent } from '../../solanaThresholdSigner/retryRequested';

export const thresholdSignerRetryRequestedEvent = {
  Bitcoin: bitcoinThresholdSignerRetryRequestedEvent,
  Evm: evmThresholdSignerRetryRequestedEvent,
  Polkadot: polkadotThresholdSignerRetryRequestedEvent,
  Solana: solanaThresholdSignerRetryRequestedEvent,
} as const;
