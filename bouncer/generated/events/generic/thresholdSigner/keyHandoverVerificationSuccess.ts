import { evmThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../evmThresholdSigner/keyHandoverVerificationSuccess';
import { polkadotThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../polkadotThresholdSigner/keyHandoverVerificationSuccess';
import { bitcoinThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../bitcoinThresholdSigner/keyHandoverVerificationSuccess';
import { solanaThresholdSignerKeyHandoverVerificationSuccessEvent } from '../../solanaThresholdSigner/keyHandoverVerificationSuccess';

export const thresholdSignerKeyHandoverVerificationSuccessEvent = {
  Arbitrum: evmThresholdSignerKeyHandoverVerificationSuccessEvent,
  Assethub: polkadotThresholdSignerKeyHandoverVerificationSuccessEvent,
  Bitcoin: bitcoinThresholdSignerKeyHandoverVerificationSuccessEvent,
  Ethereum: evmThresholdSignerKeyHandoverVerificationSuccessEvent,
  Polkadot: polkadotThresholdSignerKeyHandoverVerificationSuccessEvent,
  Solana: solanaThresholdSignerKeyHandoverVerificationSuccessEvent,
} as const;
