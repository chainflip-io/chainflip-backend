import { bitcoinThresholdSignerKeyHandoverFailureEvent } from '../../bitcoinThresholdSigner/keyHandoverFailure';
import { evmThresholdSignerKeyHandoverFailureEvent } from '../../evmThresholdSigner/keyHandoverFailure';
import { polkadotThresholdSignerKeyHandoverFailureEvent } from '../../polkadotThresholdSigner/keyHandoverFailure';
import { solanaThresholdSignerKeyHandoverFailureEvent } from '../../solanaThresholdSigner/keyHandoverFailure';

export const thresholdSignerKeyHandoverFailureEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverFailureEvent,
  Evm: evmThresholdSignerKeyHandoverFailureEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverFailureEvent,
  Solana: solanaThresholdSignerKeyHandoverFailureEvent,
} as const;
