import { bitcoinThresholdSignerMaxRetriesReachedForRequestEvent } from '../../bitcoinThresholdSigner/maxRetriesReachedForRequest';
import { evmThresholdSignerMaxRetriesReachedForRequestEvent } from '../../evmThresholdSigner/maxRetriesReachedForRequest';
import { polkadotThresholdSignerMaxRetriesReachedForRequestEvent } from '../../polkadotThresholdSigner/maxRetriesReachedForRequest';
import { solanaThresholdSignerMaxRetriesReachedForRequestEvent } from '../../solanaThresholdSigner/maxRetriesReachedForRequest';

export const thresholdSignerMaxRetriesReachedForRequestEvent = {
  Bitcoin: bitcoinThresholdSignerMaxRetriesReachedForRequestEvent,
  Evm: evmThresholdSignerMaxRetriesReachedForRequestEvent,
  Polkadot: polkadotThresholdSignerMaxRetriesReachedForRequestEvent,
  Solana: solanaThresholdSignerMaxRetriesReachedForRequestEvent,
} as const;
