import { evmThresholdSignerKeyHandoverFailureEvent } from '../../evmThresholdSigner/keyHandoverFailure';
import { polkadotThresholdSignerKeyHandoverFailureEvent } from '../../polkadotThresholdSigner/keyHandoverFailure';
import { bitcoinThresholdSignerKeyHandoverFailureEvent } from '../../bitcoinThresholdSigner/keyHandoverFailure';
import { solanaThresholdSignerKeyHandoverFailureEvent } from '../../solanaThresholdSigner/keyHandoverFailure';

export const thresholdSignerKeyHandoverFailureEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverFailureEvent,
  Assethub: polkadotThresholdSignerKeyHandoverFailureEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverFailureEvent,
  Ethereum: evmThresholdSignerKeyHandoverFailureEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverFailureEvent,
  Solana: solanaThresholdSignerKeyHandoverFailureEvent,
} as const;
