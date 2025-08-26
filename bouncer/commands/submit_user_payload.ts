#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//

import { brokerMutex, createStateChainKeypair, getSolWhaleKeyPair, handleSubstrateError, runWithTimeoutAndExit } from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { sign } from "@solana/web3.js/src/utils/ed25519";

async function main() {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair('//BROKER_1');
  const whaleKeypair = getSolWhaleKeyPair();


// TODO: Try signing like this (or maybe solana web3 has some method)
// This works but we need the "wallet"
const payload = Buffer.from(
  "Hello, Solana! This is an arbitrary message to sign."
);
const hexPayload = Buffer.from(payload).toString("hex");
// Convert payload to hex for output
// const signedMessage = await pg.wallet.signMessage(payload);
const signature = sign(payload, whaleKeypair.secretKey.slice(0, 32));
const hexSignature = Buffer.from(signature).toString("hex");
console.log("Payload (hex):", hexPayload);
console.log(
  "Signed Message (hex):",
  hexSignature
);
const hexSigner = whaleKeypair.publicKey.toBuffer().toString("hex");
console.log("Signer (hex):", hexSigner);

await brokerMutex.runExclusive(async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload(
        // Serialized data - this is an example of a Message of a transaction
        "0x"+hexPayload,
        // Signature over the payload
        "0x"+hexSignature,
        // signer
        "0x"+hexSigner
      )
      .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  });

// await brokerMutex.runExclusive(async () => {
//     const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
//     await chainflip.tx.swapping
//       .submitUserSignedPayload(
//         // Serialized data - this is an example of a Message of a transaction
//         "0x010000023f6c2b3023f64ac0c2a7775c2b0725d62d5c075513f122728488f04b73c92ab70000000000000000000000000000000000000000000000000000000000000000c949177e6404531fc14eab9b2b5aae0849da386d8f0c687f98ecfee4aea4d9cb01010200010c02000000ff970d0000000000",
//         // Signature over the payload
//         "0xbd5a83a3dd9e2a5c463ab47df52234971cc6725e2abe1cdd0d9b962554f465a61adce10d6ca3c7a327d8221b5b53008a70148288ebe2ca92780b43fcae3b6700",
//         // signer
//         "0x3f6c2b3023f64ac0c2a7775c2b0725d62d5c075513f122728488f04b73c92ab7"
//       )
//       .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
//   });
}

await runWithTimeoutAndExit(main(), 20);
