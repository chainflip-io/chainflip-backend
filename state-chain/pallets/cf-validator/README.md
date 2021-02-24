# How to add a validator

1. Start 2 nodes
2. Ensure each node has their Aura and Grandpa keys set. You set keys by using the `author` insertKey RPC call on the polkadot js app. Follow this guide from here https://substrate.dev/docs/en/tutorials/start-a-private-network/keygen
3. NB: Make sure that the first of the keys you generate/use, are entered into the chainspec correctly. SR25519 pubkeys for Aura, and ED25519 pubkeys for Grandpa.
If you've done the above correctly, you should start authoring blocks. However, if finalisation is not occurring, you will have to restart the node that was configured as an an authority in the genesis block.
4. After finalisation is occurring go to RPC -> author -> rotateKeys() and copy the key returned.
5. Go to accounts and create an account to represent the second node. Note: This will be the same seed that was used to submit the second key to the node. This will be Account B.
6. Go to extrinsics, set the user as Account B and then session -> setKeys().
Paste the rotated key from step 4 into the `keys` field.
Enter `0x00` into the `proof` field
Submit the signed transaction.
7. Now, still in Extrinsics and still on Account B, you can enter: validator -> addValidator, and add Account B as validator.

Now when you return to the block explorer you should see both Accounts/Nodes producing blocks, and finalisation should be following. If finalisation is stuck, you may need to restart the second node so it uses its new keys.