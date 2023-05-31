#!/bin/bash
cd ../chainflip-backend
printf "3\n" | ./localnet/manage.sh > /dev/null 2> /dev/null
echo "localnet terminated"