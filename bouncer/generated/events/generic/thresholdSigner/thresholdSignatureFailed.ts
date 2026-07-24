import { evmThresholdSignerThresholdSignatureFailedEvent } from '../../evmThresholdSigner/thresholdSignatureFailed';
import { polkadotThresholdSignerThresholdSignatureFailedEvent } from '../../polkadotThresholdSigner/thresholdSignatureFailed';
import { bitcoinThresholdSignerThresholdSignatureFailedEvent } from '../../bitcoinThresholdSigner/thresholdSignatureFailed';
import { solanaThresholdSignerThresholdSignatureFailedEvent } from '../../solanaThresholdSigner/thresholdSignatureFailed';

export const thresholdSignerThresholdSignatureFailedEvent = {
  Arbitrum: evmThresholdSignerThresholdSignatureFailedEvent,
  Assethub: polkadotThresholdSignerThresholdSignatureFailedEvent,
  Bitcoin: bitcoinThresholdSignerThresholdSignatureFailedEvent,
  Ethereum: evmThresholdSignerThresholdSignatureFailedEvent,
  Polkadot: polkadotThresholdSignerThresholdSignatureFailedEvent,
  Solana: solanaThresholdSignerThresholdSignatureFailedEvent,
} as const;
