import { bitcoinThresholdSignerKeyHandoverVerificationFailureEvent } from '../../bitcoinThresholdSigner/keyHandoverVerificationFailure';
import { evmThresholdSignerKeyHandoverVerificationFailureEvent } from '../../evmThresholdSigner/keyHandoverVerificationFailure';
import { polkadotThresholdSignerKeyHandoverVerificationFailureEvent } from '../../polkadotThresholdSigner/keyHandoverVerificationFailure';
import { solanaThresholdSignerKeyHandoverVerificationFailureEvent } from '../../solanaThresholdSigner/keyHandoverVerificationFailure';

export const thresholdSignerKeyHandoverVerificationFailureEvent = {
  Bitcoin: bitcoinThresholdSignerKeyHandoverVerificationFailureEvent,
  Evm: evmThresholdSignerKeyHandoverVerificationFailureEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverVerificationFailureEvent,
  Solana: solanaThresholdSignerKeyHandoverVerificationFailureEvent,
} as const;
