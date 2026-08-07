import {
  Asset,
  amountToFineAmountBigInt,
  assetDecimals,
  chainFromAsset,
  fineAmountToAmount,
} from 'shared/utils';
import { CHAINFLIP_HTTP_ENDPOINT, getChainflipApi } from 'shared/utils/substrate';
import { Logger } from 'shared/utils/logger';
import { jsonRpc } from 'shared/json_rpc';

// Network gating for running bouncer commands against a live network.
// Everything here is deliberately opt-in via BOUNCER_NETWORK: with the variable unset the
// bouncer behaves exactly as before (localnet mode).

export type BouncerNetwork = 'localnet' | 'perseverance';

export const KNOWN_GENESIS_HASHES: Record<string, string> = {
  berghain: '0x8b8c140b0af9db70686583e3f6bf2a59052bfe9584b97d20c45068281e976eb9',
  perseverance: '0x7a5d4db858ada1d20ed6ded4933c33313fc9673e5fffab560d0ca714782f2080',
  sisyphos: '0x4c5328ad95cedeb3c89e24edd12cb687d950fd3da8559358dc474f0ddd9a3f99',
};

export function getBouncerNetwork(): BouncerNetwork {
  const network = process.env.BOUNCER_NETWORK ?? 'localnet';
  if (network !== 'localnet' && network !== 'perseverance') {
    throw new Error(
      `Unsupported BOUNCER_NETWORK '${network}', expected 'localnet' or 'perseverance'`,
    );
  }
  return network;
}

export function isLiveNetwork(): boolean {
  return getBouncerNetwork() !== 'localnet';
}

/**
 * Ensures all environment variables needed for a live-network run are explicitly set, so we
 * never silently fall back to a localhost default while talking to a live network. Reports
 * every missing variable at once. No-op on localnet.
 */
export function requireLiveEnv(logger: Logger, extraVars: string[] = []) {
  if (!isLiveNetwork()) {
    return;
  }
  // Indexer access and the node HTTP endpoint need no variables: event queries default to the
  // public indexer-gateway of the selected network (override with INDEXER_GATEWAY_URL, see
  // shared/utils/indexer_db.ts), and one-shot RPC calls derive an HTTP endpoint from
  // CF_NODE_ENDPOINT (override with CF_NODE_HTTP_ENDPOINT).
  const requiredVars = [...new Set(['CF_NODE_ENDPOINT', ...extraVars])];
  const missing = requiredVars.filter((name) => !process.env[name]);
  if (missing.length > 0) {
    throw new Error(
      `BOUNCER_NETWORK=${getBouncerNetwork()} requires the following environment variables to be set: ${missing.join(', ')}`,
    );
  }
  logger.debug(`Live env check passed for: ${requiredVars.join(', ')}`);
}

/**
 * Environment variables needed to interact with the external chain side of `asset` (send funds,
 * read balances). On a live network the localnet contract-address defaults are wrong, so ERC20
 * contract addresses must be provided explicitly.
 */
export function requiredEnvForAsset(asset: Asset): string[] {
  const chainEndpointVars: Partial<Record<string, string[]>> = {
    Ethereum: ['ETH_ENDPOINT'],
    Arbitrum: ['ARB_ENDPOINT'],
    Solana: ['SOL_HTTP_ENDPOINT', 'SOL_WS_ENDPOINT'],
    Polkadot: ['POLKADOT_ENDPOINT'],
    Assethub: ['ASSETHUB_ENDPOINT'],
    Bitcoin: ['BTC_ENDPOINT'],
    Bsc: ['BSC_ENDPOINT'],
  };
  const erc20AddressVars: Partial<Record<Asset, string[]>> = {
    Usdc: ['ETH_USDC_ADDRESS'],
    Usdt: ['ETH_USDT_ADDRESS'],
    Flip: ['ETH_FLIP_ADDRESS'],
    Wbtc: ['ETH_WBTC_ADDRESS'],
    ArbUsdc: ['ARB_USDC_ADDRESS'],
    ArbUsdt: ['ARB_USDT_ADDRESS'],
  };
  return [...(chainEndpointVars[chainFromAsset(asset)] ?? []), ...(erc20AddressVars[asset] ?? [])];
}

/**
 * Verifies that the node we are connected to is the network we think it is, by genesis hash.
 * In live mode the genesis must match the selected network exactly; in localnet mode it must
 * NOT be any known public network. Mainnet (berghain) can never pass either check.
 */
export async function assertExpectedNetwork(logger: Logger): Promise<string> {
  const endpoint = CHAINFLIP_HTTP_ENDPOINT;
  const genesisHash = (await jsonRpc(logger, 'chain_getBlockHash', [0], endpoint)) as unknown;
  if (typeof genesisHash !== 'string' || !genesisHash.startsWith('0x')) {
    throw new Error(
      `Could not read the genesis hash from ${endpoint} (got ${JSON.stringify(genesisHash)}). Refusing to run.`,
    );
  }
  const network = getBouncerNetwork();

  const knownNetwork = Object.entries(KNOWN_GENESIS_HASHES).find(
    ([, hash]) => hash === genesisHash,
  )?.[0];

  if (network === 'localnet') {
    if (knownNetwork !== undefined) {
      throw new Error(
        `BOUNCER_NETWORK=localnet but ${endpoint} is the public '${knownNetwork}' network (genesis ${genesisHash}). Refusing to run.`,
      );
    }
  } else if (genesisHash !== KNOWN_GENESIS_HASHES[network]) {
    throw new Error(
      `BOUNCER_NETWORK=${network} but ${endpoint} has genesis ${genesisHash}` +
        (knownNetwork ? ` which is the '${knownNetwork}' network. ` : '. ') +
        `Expected ${KNOWN_GENESIS_HASHES[network]}. Refusing to run.`,
    );
  }
  logger.info(`Connected to ${network} (genesis ${genesisHash})`);
  return genesisHash;
}

export type LiveTimeouts = {
  /** Deposit sent on the external chain -> witnessed on the state chain. */
  depositWitnessSeconds: number;
  /** SwapRequested -> SwapRequestCompleted (covers DCA chunks and FoK retries). */
  swapCompletionSeconds: number;
  /** Egress scheduled -> broadcast success -> destination balance increase. */
  egressSeconds: number;
  /** Whole command, used with runWithTimeoutAndExit. */
  totalRunSeconds: number;
};

export function networkTimeouts(): LiveTimeouts {
  if (isLiveNetwork()) {
    // Ceilings are sized for EVM-chain pairs (a BTC leg would need per-chain values). The
    // first Perseverance run measured ~35s witnessing, ~12s completion, ~85s egress; the
    // margins absorb slow external chains and FoK retries (50 blocks = 300s).
    return {
      depositWitnessSeconds: 1800,
      swapCompletionSeconds: 900,
      egressSeconds: 1800,
      totalRunSeconds: 4800,
    };
  }
  return {
    depositWitnessSeconds: 120,
    swapCompletionSeconds: 120,
    egressSeconds: 200,
    totalRunSeconds: 600,
  };
}

// Conservative per-asset caps on the amount a live swap command may move.
const DEFAULT_MAX_SWAP_AMOUNTS: Partial<Record<Asset, number>> = {
  Eth: 0.01,
  ArbEth: 0.01,
  Usdc: 25,
  Usdt: 25,
  ArbUsdc: 25,
  ArbUsdt: 25,
  Flip: 100,
  Sol: 0.5,
  SolUsdc: 25,
  SolUsdt: 25,
  HubDot: 5,
  HubUsdc: 25,
  HubUsdt: 25,
};

export function maxAllowedSwapAmount(asset: Asset): number {
  const cap = DEFAULT_MAX_SWAP_AMOUNTS[asset];
  if (cap === undefined) {
    throw new Error(`No amount cap configured for ${asset}; refusing to swap it on a live network`);
  }
  return cap;
}

/** Refuses amounts above the configured per-asset cap. No-op on localnet. */
export function assertAmountAllowed(asset: Asset, amount: number) {
  if (!isLiveNetwork()) {
    return;
  }
  const cap = maxAllowedSwapAmount(asset);
  if (!(amount > 0) || amount > cap) {
    throw new Error(
      `Amount ${amount} ${asset} is outside the allowed live-network range (0, ${cap}].`,
    );
  }
}

/** The protocol's minimum deposit for `asset`, in fine units. */
export async function minimumDepositAmount(asset: Asset): Promise<bigint> {
  await using client = await getChainflipApi();
  return client.call.customRuntimeApi.cfMinDepositAmount(asset);
}

/**
 * Refuses a deposit below the protocol minimum. A sub-minimum deposit is ignored on-chain (no
 * SwapRequested / AccountCredited fires), so the funds are spent but the command just hangs until
 * timeout. No-op on localnet, where the minimum is zero.
 */
export async function assertAboveMinimumDeposit(logger: Logger, asset: Asset, amount: number) {
  if (!isLiveNetwork()) {
    return;
  }
  const min = await minimumDepositAmount(asset);
  if (amountToFineAmountBigInt(String(amount), asset) < min) {
    throw new Error(
      `Amount ${amount} ${asset} is below the protocol minimum deposit of ` +
        `${fineAmountToAmount(min.toString(), assetDecimals(asset))} ${asset}; it would be ignored on-chain.`,
    );
  }
  logger.debug(`${amount} ${asset} is above the minimum deposit`);
}
