import { bitcoinThresholdSignerNoKeyHandoverEvent } from '../../bitcoinThresholdSigner/noKeyHandover';
import { evmThresholdSignerNoKeyHandoverEvent } from '../../evmThresholdSigner/noKeyHandover';
import { polkadotThresholdSignerNoKeyHandoverEvent } from '../../polkadotThresholdSigner/noKeyHandover';
import { solanaThresholdSignerNoKeyHandoverEvent } from '../../solanaThresholdSigner/noKeyHandover';

export const thresholdSignerNoKeyHandoverEvent = {
  Bitcoin: bitcoinThresholdSignerNoKeyHandoverEvent,
  Evm: evmThresholdSignerNoKeyHandoverEvent,
  Polkadot: polkadotThresholdSignerNoKeyHandoverEvent,
  Solana: solanaThresholdSignerNoKeyHandoverEvent,
} as const;
