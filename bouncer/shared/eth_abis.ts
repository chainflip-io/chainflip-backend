
import fs from 'fs/promises';

async function loadContract(abiPath: string): Promise<JSON> {
    const abi = await fs.readFile(abiPath, 'utf-8');
    return JSON.parse(abi);
}

export const erc20abi = await loadContract('../eth-contract-abis/IERC20.json');
export const cfReceiverMockAbi = await loadContract('../eth-contract-abis/perseverance-rc17/CFReceiverMock.json');
