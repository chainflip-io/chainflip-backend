import { evmThresholdSignerThresholdSignatureSuccessEvent } from '../../evmThresholdSigner/thresholdSignatureSuccess';
import { polkadotThresholdSignerThresholdSignatureSuccessEvent } from '../../polkadotThresholdSigner/thresholdSignatureSuccess';
import { bitcoinThresholdSignerThresholdSignatureSuccessEvent } from '../../bitcoinThresholdSigner/thresholdSignatureSuccess';
import { solanaThresholdSignerThresholdSignatureSuccessEvent } from '../../solanaThresholdSigner/thresholdSignatureSuccess';

export const thresholdSignerThresholdSignatureSuccessEvent = {
  Arbitrum: evmThresholdSignerThresholdSignatureSuccessEvent,
  Assethub: polkadotThresholdSignerThresholdSignatureSuccessEvent,
  Bitcoin: bitcoinThresholdSignerThresholdSignatureSuccessEvent,
  Ethereum: evmThresholdSignerThresholdSignatureSuccessEvent,
  Polkadot: polkadotThresholdSignerThresholdSignatureSuccessEvent,
  Solana: solanaThresholdSignerThresholdSignatureSuccessEvent,
} as const;
