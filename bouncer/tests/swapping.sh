#!/bin/bash

echo "=== Testing all swap combinations ===" &&
MY_ADDRESS=`pnpm tsx ./commands/new_btc_address.ts never P2PKH`
echo "Created new BTC address $MY_ADDRESS"
pnpm tsx ./commands/perform_swap.ts dot btc $MY_ADDRESS
# MY_ADDRESS=`pnpm tsx ./commands/new_btc_address.ts gonna P2SH` &&
# echo "Created new BTC address $MY_ADDRESS" &&
# perform_swap eth btc $MY_ADDRESS &&
# MY_ADDRESS=`pnpm tsx ./commands/new_btc_address.ts give P2WPKH` &&
# echo "Created new BTC address $MY_ADDRESS" &&
# perform_swap usdc btc $MY_ADDRESS &&
# MY_ADDRESS=`pnpm tsx ./commands/new_btc_address.ts you P2WSH` &&
# echo "Created new BTC address $MY_ADDRESS" &&
# perform_swap dot btc $MY_ADDRESS &&
# MY_ADDRESS=`pnpm tsx ./commands/new_dot_address.ts up` &&
# echo "Created new DOT address $MY_ADDRESS" &&
# perform_swap btc dot $MY_ADDRESS &&
# MY_ADDRESS=`pnpm tsx ./commands/new_eth_address.ts and` &&
# echo "Created new USDC address $MY_ADDRESS" &&
# perform_swap dot usdc $MY_ADDRESS
# MY_ADDRESS=`pnpm tsx ./commands/new_eth_address.ts desert` &&
# echo "Created new ETH address $MY_ADDRESS" &&
# perform_swap btc eth $MY_ADDRESS &&
# echo "=== Test complete ==="
