import type { Argv } from 'yargs';

// Public ws JSON-RPC endpoints for the named Chainflip networks, shared by the read-only CLI
// commands that accept a `--network` flag (query_storage, oracle_prices, ...). `--endpoint`
// overrides these. The state chain node serves both HTTP and WS on the same RPC port, so callers
// that need HTTP can swap the ws(s):// scheme for http(s)://.
export const NETWORKS: Record<string, string> = {
  localnet: 'ws://127.0.0.1:9944',
  mainnet: 'wss://mainnet-rpc.chainflip.io',
  berghain: 'wss://mainnet-rpc.chainflip.io',
  perseverance: 'wss://perseverance.chainflip.xyz',
  sisyphos: 'wss://archive.sisyphos.chainflip.io',
};

// Resolve a `--network` name to its ws endpoint, throwing a helpful error on an unknown name.
export function networkWsEndpoint(network: string): string {
  const endpoint = NETWORKS[network.toLowerCase()];
  if (!endpoint) {
    throw new Error(
      `Unknown network '${network}'. Known: ${Object.keys(NETWORKS).join(', ')}. ` +
        `Or pass --endpoint <url>.`,
    );
  }
  return endpoint;
}

// Resolve the ws(s) JSON-RPC endpoint from CLI-style selection (precedence: explicit endpoint, a
// named network, then CF_NODE_ENDPOINT / localnet). This is the single source of truth for the
// `--endpoint` > `--network` > env selection ladder shared by the read-only commands.
export function resolveWsEndpoint(opts: { endpoint?: string; network?: string }): string {
  if (opts.endpoint) {
    return opts.endpoint;
  }
  if (opts.network) {
    return networkWsEndpoint(opts.network);
  }
  return process.env.CF_NODE_ENDPOINT ?? NETWORKS.localnet;
}

// As {@link resolveWsEndpoint} but in http(s) form. The node serves HTTP and WS on the same RPC
// port, so the result is usable directly for one-shot `fetch()` JSON-RPC POSTs.
export function resolveHttpEndpoint(opts: { endpoint?: string; network?: string }): string {
  return resolveWsEndpoint(opts)
    .replace(/^wss:\/\//, 'https://')
    .replace(/^ws:\/\//, 'http://');
}

// Apply the shared `--network` / `--endpoint` selection flags to a yargs builder, so every
// read-only command exposes them identically.
export function withNetworkOptions<T>(y: Argv<T>) {
  return y
    .option('network', {
      type: 'string',
      describe: `Named network (${Object.keys(NETWORKS).join('|')})`,
    })
    .option('endpoint', {
      type: 'string',
      describe: 'Custom ws(s) endpoint (overrides --network)',
    });
}
