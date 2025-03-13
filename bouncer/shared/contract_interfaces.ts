import fs from 'fs/promises';

async function loadContract(abiPath: string): Promise<ReturnType<typeof JSON.parse>> {
  const abi = await fs.readFile(abiPath, 'utf-8');
  return JSON.parse(abi);
}

function loadContractCached(abiPath: string) {
  let cached: ReturnType<typeof JSON.parse> | undefined;
  return async () => {
    cached ??= await loadContract(abiPath);
    return cached;
  };
}
const CF_ETH_CONTRACT_ABI_TAG = 'v1.1.2';
const CF_SOL_PROGRAM_IDL_TAG = 'v1.0.1-swap-endpoint';
export const getErc20abi = loadContractCached(
  '../contract-interfaces/eth-contract-abis/IERC20.json',
);
export const getGatewayAbi = loadContractCached(
  `../contract-interfaces/eth-contract-abis/${CF_ETH_CONTRACT_ABI_TAG}/IStateChainGateway.json`,
);
export const getCFTesterAbi = loadContractCached(
  `../contract-interfaces/eth-contract-abis/${CF_ETH_CONTRACT_ABI_TAG}/CFTester.json`,
);
export const getKeyManagerAbi = loadContractCached(
  `../contract-interfaces/eth-contract-abis/${CF_ETH_CONTRACT_ABI_TAG}/IKeyManager.json`,
);
export const getEvmVaultAbi = loadContractCached(
  `../contract-interfaces/eth-contract-abis/${CF_ETH_CONTRACT_ABI_TAG}/IVault.json`,
);
export const getSolanaVaultIdl = loadContractCached(
  `../contract-interfaces/sol-program-idls/${CF_SOL_PROGRAM_IDL_TAG}/vault.json`,
);
export const getCfTesterIdl = loadContractCached(
  `../contract-interfaces/sol-program-idls/${CF_SOL_PROGRAM_IDL_TAG}/cf_tester.json`,
);
export const getSolanaSwapEndpointIdl = loadContractCached(
  `../contract-interfaces/sol-program-idls/${CF_SOL_PROGRAM_IDL_TAG}/swap_endpoint.json`,
);
