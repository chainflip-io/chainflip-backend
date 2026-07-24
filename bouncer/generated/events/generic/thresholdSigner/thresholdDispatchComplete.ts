import { bitcoinThresholdSignerThresholdDispatchCompleteEvent } from '../../bitcoinThresholdSigner/thresholdDispatchComplete';
import { evmThresholdSignerThresholdDispatchCompleteEvent } from '../../evmThresholdSigner/thresholdDispatchComplete';
import { polkadotThresholdSignerThresholdDispatchCompleteEvent } from '../../polkadotThresholdSigner/thresholdDispatchComplete';
import { solanaThresholdSignerThresholdDispatchCompleteEvent } from '../../solanaThresholdSigner/thresholdDispatchComplete';

export const thresholdSignerThresholdDispatchCompleteEvent = {
  Bitcoin: bitcoinThresholdSignerThresholdDispatchCompleteEvent,
  Evm: evmThresholdSignerThresholdDispatchCompleteEvent,
  Polkadot: polkadotThresholdSignerThresholdDispatchCompleteEvent,
  Solana: solanaThresholdSignerThresholdDispatchCompleteEvent,
} as const;
