import { evmThresholdSignerNoKeyHandoverEvent } from '../../evmThresholdSigner/noKeyHandover';
import { polkadotThresholdSignerNoKeyHandoverEvent } from '../../polkadotThresholdSigner/noKeyHandover';
import { bitcoinThresholdSignerNoKeyHandoverEvent } from '../../bitcoinThresholdSigner/noKeyHandover';
import { solanaThresholdSignerNoKeyHandoverEvent } from '../../solanaThresholdSigner/noKeyHandover';

export const thresholdSignerNoKeyHandoverEvent = {
  Arbitrum: evmThresholdSignerNoKeyHandoverEvent,
  Assethub: polkadotThresholdSignerNoKeyHandoverEvent,
  Bitcoin: bitcoinThresholdSignerNoKeyHandoverEvent,
  Ethereum: evmThresholdSignerNoKeyHandoverEvent,
  Polkadot: polkadotThresholdSignerNoKeyHandoverEvent,
  Solana: solanaThresholdSignerNoKeyHandoverEvent,
} as const;
