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

// Resolve the http(s) JSON-RPC endpoint from CLI-style selection (precedence: explicit endpoint, a
// named network, then CF_NODE_ENDPOINT / localnet), returned in http(s) form. The node serves HTTP
// and WS on the same RPC port, so the result is usable directly for one-shot `fetch()` JSON-RPC POSTs.
export function resolveHttpEndpoint(opts: { endpoint?: string; network?: string }): string {
  let ws: string;
  if (opts.endpoint) {
    ws = opts.endpoint;
  } else if (opts.network) {
    ws = networkWsEndpoint(opts.network);
  } else {
    ws = process.env.CF_NODE_ENDPOINT ?? NETWORKS.localnet;
  }
  return ws.replace(/^wss:\/\//, 'https://').replace(/^ws:\/\//, 'http://');
}
