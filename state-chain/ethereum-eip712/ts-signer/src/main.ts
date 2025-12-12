import { ethers } from 'ethers';
import { hashTypedData } from 'viem';
import { SignTypedDataVersion, TypedDataUtils } from '@metamask/eth-sig-util';

type Library = 'ethers' | 'viem' | 'eth-sig-util';

interface EIP712Domain {
  name?: string;
  version?: string;
  chainId?: number | string;
  verifyingContract?: string;
  salt?: string;
}

interface EIP712TypedData {
  types: Record<string, { name: string; type: string }[]>;
  primaryType: string;
  domain: EIP712Domain;
  message: Record<string, unknown>;
}

interface HashResult {
  library: Library;
  signingHash: string;
}

interface ErrorResult {
  error: string;
}

function hashWithEthers(typedData: EIP712TypedData): HashResult {
  const { EIP712Domain: _, ...types } = typedData.types;

  // Normalize chainId to bigint if present
  const domain =
    typedData.domain.chainId !== undefined
      ? { ...typedData.domain, chainId: BigInt(typedData.domain.chainId) }
      : typedData.domain;

  const signingHash = ethers.TypedDataEncoder.hash(domain, types, typedData.message);

  return { library: 'ethers', signingHash };
}

function hashWithViem(typedData: EIP712TypedData): HashResult {
  const { EIP712Domain: _, ...types } = typedData.types;

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const signingHash = hashTypedData({
    domain: typedData.domain as Parameters<typeof hashTypedData>[0]['domain'],
    types,
    primaryType: typedData.primaryType,
    message: typedData.message as Record<string, unknown>,
  });

  return { library: 'viem', signingHash };
}

function hashWithEthSigUtil(typedData: EIP712TypedData): HashResult {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const hash = TypedDataUtils.eip712Hash(typedData as any, SignTypedDataVersion.V4);
  const signingHash = `0x${hash.toString('hex')}`;

  return { library: 'eth-sig-util', signingHash };
}

const LIBRARIES = ['ethers', 'viem', 'eth-sig-util'] as const;

function printUsage() {
  console.error('Usage: eip712-signer --library <library>');
  console.error('');
  console.error('Options:');
  console.error('  --library, -l  Library to use for hashing');
  console.error(`                 Choices: ${LIBRARIES.join(', ')}`);
  console.error('');
  console.error('Input: JSON via stdin with EIP-712 typed data');
}

async function main() {
  const args = process.argv.slice(2);

  let library: Library | undefined;

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--library' || args[i] === '-l') {
      library = args[++i] as Library;
    } else if (args[i] === '--help' || args[i] === '-h') {
      printUsage();
      process.exit(0);
    }
  }

  if (!library) {
    const error: ErrorResult = { error: 'Missing required argument: --library' };
    console.log(JSON.stringify(error));
    process.exit(1);
  }

  if (!LIBRARIES.includes(library)) {
    const error: ErrorResult = {
      error: `Invalid library: ${library}. Must be one of: ${LIBRARIES.join(', ')}`,
    };
    console.log(JSON.stringify(error));
    process.exit(1);
  }

  // Read stdin - works with both Bun and Node.js
  const input = await new Promise<string>((resolve) => {
    let data = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', (chunk) => {
      data += chunk;
    });
    process.stdin.on('end', () => {
      resolve(data);
    });
  });

  let typedData: EIP712TypedData;
  try {
    typedData = JSON.parse(input);
  } catch {
    const error: ErrorResult = { error: 'Invalid JSON input' };
    console.log(JSON.stringify(error));
    process.exit(1);
  }

  try {
    let result: HashResult;

    switch (library) {
      case 'ethers':
        result = hashWithEthers(typedData);
        break;
      case 'viem':
        result = hashWithViem(typedData);
        break;
      case 'eth-sig-util':
        result = hashWithEthSigUtil(typedData);
        break;
      default:
        throw new Error(`Unknown library: ${library}`);
    }

    console.log(JSON.stringify(result));
  } catch (e) {
    const error: ErrorResult = {
      error: e instanceof Error ? e.message : String(e),
    };
    console.log(JSON.stringify(error));
    process.exit(1);
  }
}

main();
