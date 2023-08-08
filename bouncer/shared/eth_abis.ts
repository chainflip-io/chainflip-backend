import fs from 'fs/promises';

async function loadContract(abiPath: string): Promise<JSON> {
  const abi = await fs.readFile(abiPath, 'utf-8');
  return JSON.parse(abi);
}

function loadContractCached(abiPath: string) {
  let cached: JSON | undefined;
  return async () => {
    if (!cached) {
      cached = await loadContract(abiPath);
    }
    return cached;
  };
}

export const getErc20abi = loadContractCached('../eth-contract-abis/IERC20.json');
export const getGatewayAbi = loadContractCached(
  '../eth-contract-abis/perseverance-0.9-rc3/IStateChainGateway.json',
);
export const getCFTesterAbi = loadContractCached(
  '../eth-contract-abis/perseverance-0.9-rc3/CFTester.json',
);
