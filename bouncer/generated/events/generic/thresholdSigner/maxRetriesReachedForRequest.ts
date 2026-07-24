import { evmThresholdSignerMaxRetriesReachedForRequestEvent } from '../../evmThresholdSigner/maxRetriesReachedForRequest';
import { polkadotThresholdSignerMaxRetriesReachedForRequestEvent } from '../../polkadotThresholdSigner/maxRetriesReachedForRequest';
import { bitcoinThresholdSignerMaxRetriesReachedForRequestEvent } from '../../bitcoinThresholdSigner/maxRetriesReachedForRequest';
import { solanaThresholdSignerMaxRetriesReachedForRequestEvent } from '../../solanaThresholdSigner/maxRetriesReachedForRequest';

export const thresholdSignerMaxRetriesReachedForRequestEvent = {
  Arbitrum: evmThresholdSignerMaxRetriesReachedForRequestEvent,
  Assethub: polkadotThresholdSignerMaxRetriesReachedForRequestEvent,
  Bitcoin: bitcoinThresholdSignerMaxRetriesReachedForRequestEvent,
  Ethereum: evmThresholdSignerMaxRetriesReachedForRequestEvent,
  Polkadot: polkadotThresholdSignerMaxRetriesReachedForRequestEvent,
  Solana: solanaThresholdSignerMaxRetriesReachedForRequestEvent,
} as const;
