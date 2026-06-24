import { evmThresholdSignerKeyHandoverVerificationFailureEvent } from '../../evmThresholdSigner/keyHandoverVerificationFailure';
import { polkadotThresholdSignerKeyHandoverVerificationFailureEvent } from '../../polkadotThresholdSigner/keyHandoverVerificationFailure';
import { bitcoinThresholdSignerKeyHandoverVerificationFailureEvent } from '../../bitcoinThresholdSigner/keyHandoverVerificationFailure';
import { solanaThresholdSignerKeyHandoverVerificationFailureEvent } from '../../solanaThresholdSigner/keyHandoverVerificationFailure';

export const thresholdSignerKeyHandoverVerificationFailureEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverVerificationFailureEvent,
  Assethub: polkadotThresholdSignerKeyHandoverVerificationFailureEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverVerificationFailureEvent,
  Ethereum: evmThresholdSignerKeyHandoverVerificationFailureEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverVerificationFailureEvent,
  Solana: solanaThresholdSignerKeyHandoverVerificationFailureEvent,
} as const;
