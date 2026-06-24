import { evmThresholdSignerKeygenVerificationSuccessEvent } from '../../evmThresholdSigner/keygenVerificationSuccess';
import { polkadotThresholdSignerKeygenVerificationSuccessEvent } from '../../polkadotThresholdSigner/keygenVerificationSuccess';
import { bitcoinThresholdSignerKeygenVerificationSuccessEvent } from '../../bitcoinThresholdSigner/keygenVerificationSuccess';
import { solanaThresholdSignerKeygenVerificationSuccessEvent } from '../../solanaThresholdSigner/keygenVerificationSuccess';

export const thresholdSignerKeygenVerificationSuccessEvent = {
  Arbitrum: evmThresholdSignerKeygenVerificationSuccessEvent,
  Assethub: polkadotThresholdSignerKeygenVerificationSuccessEvent,
  Bitcoin: bitcoinThresholdSignerKeygenVerificationSuccessEvent,
  Ethereum: evmThresholdSignerKeygenVerificationSuccessEvent,
  Polkadot: polkadotThresholdSignerKeygenVerificationSuccessEvent,
  Solana: solanaThresholdSignerKeygenVerificationSuccessEvent,
} as const;
