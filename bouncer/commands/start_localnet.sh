#!/bin/bash
cd ../
printf "1\n./target/debug\n" | ./localnet/manage.sh  > /dev/null 2> /dev/null
echo "localnet started"
