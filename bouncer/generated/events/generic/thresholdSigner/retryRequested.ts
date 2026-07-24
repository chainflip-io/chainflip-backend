import { evmThresholdSignerRetryRequestedEvent } from '../../evmThresholdSigner/retryRequested';
import { polkadotThresholdSignerRetryRequestedEvent } from '../../polkadotThresholdSigner/retryRequested';
import { bitcoinThresholdSignerRetryRequestedEvent } from '../../bitcoinThresholdSigner/retryRequested';
import { solanaThresholdSignerRetryRequestedEvent } from '../../solanaThresholdSigner/retryRequested';

export const thresholdSignerRetryRequestedEvent = {
  Arbitrum: evmThresholdSignerRetryRequestedEvent,
  Assethub: polkadotThresholdSignerRetryRequestedEvent,
  Bitcoin: bitcoinThresholdSignerRetryRequestedEvent,
  Ethereum: evmThresholdSignerRetryRequestedEvent,
  Polkadot: polkadotThresholdSignerRetryRequestedEvent,
  Solana: solanaThresholdSignerRetryRequestedEvent,
} as const;
