import { evmThresholdSignerThresholdSignatureRequestEvent } from '../../evmThresholdSigner/thresholdSignatureRequest';
import { polkadotThresholdSignerThresholdSignatureRequestEvent } from '../../polkadotThresholdSigner/thresholdSignatureRequest';
import { bitcoinThresholdSignerThresholdSignatureRequestEvent } from '../../bitcoinThresholdSigner/thresholdSignatureRequest';
import { solanaThresholdSignerThresholdSignatureRequestEvent } from '../../solanaThresholdSigner/thresholdSignatureRequest';

export const thresholdSignerThresholdSignatureRequestEvent = {
  Arbitrum: evmThresholdSignerThresholdSignatureRequestEvent,
  Assethub: polkadotThresholdSignerThresholdSignatureRequestEvent,
  Bitcoin: bitcoinThresholdSignerThresholdSignatureRequestEvent,
  Ethereum: evmThresholdSignerThresholdSignatureRequestEvent,
  Polkadot: polkadotThresholdSignerThresholdSignatureRequestEvent,
  Solana: solanaThresholdSignerThresholdSignatureRequestEvent,
} as const;
