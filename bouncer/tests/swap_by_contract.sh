# Does an ETH to Dot swap by calling the Vault contract directly.

MY_ADDRESS=$(./commands/new_dot_address.sh test1)
echo "Created new DOT address $MY_ADDRESS"
node ./commands/new_swap_via_vault_contract $MY_ADDRESS 100000000000
./commands/observe_events.sh --timeout 30000 --succeed_on swapping:SwapExecuted --fail_on foo:bar > /dev/null &&
for i in {1..60}; do
    NEW_BALANCE=$(./commands/get_dot_balance $MY_ADDRESS)
    if (( NEW_BALANCE > 0 )); then
        exit 0
    else
        echo "Not found yet"
        sleep 2
    fi
done
exit 1