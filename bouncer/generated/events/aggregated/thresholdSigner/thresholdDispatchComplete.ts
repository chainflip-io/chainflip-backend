import { evmThresholdSignerThresholdDispatchCompleteEvent } from '../../evmThresholdSigner/thresholdDispatchComplete';
import { polkadotThresholdSignerThresholdDispatchCompleteEvent } from '../../polkadotThresholdSigner/thresholdDispatchComplete';
import { bitcoinThresholdSignerThresholdDispatchCompleteEvent } from '../../bitcoinThresholdSigner/thresholdDispatchComplete';
import { solanaThresholdSignerThresholdDispatchCompleteEvent } from '../../solanaThresholdSigner/thresholdDispatchComplete';

export const thresholdSignerThresholdDispatchCompleteEvent = {
  Arbitrum: evmThresholdSignerThresholdDispatchCompleteEvent,
  Assethub: polkadotThresholdSignerThresholdDispatchCompleteEvent,
  Bitcoin: bitcoinThresholdSignerThresholdDispatchCompleteEvent,
  Ethereum: evmThresholdSignerThresholdDispatchCompleteEvent,
  Polkadot: polkadotThresholdSignerThresholdDispatchCompleteEvent,
  Solana: solanaThresholdSignerThresholdDispatchCompleteEvent,
} as const;
