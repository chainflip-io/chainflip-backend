import { sha256 } from '../shared/utils';
import { Keypair } from "@solana/web3.js";

export function newSolAddress(seed: string): string {
    return Keypair.fromSeed(sha256(seed)).publicKey.toBase58()
}
