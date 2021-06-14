# This would be repeated for each node in our testnet
# Assumes a node is listening for JSON RPC @ port 9933 locally
# For each json file we would provide the `suri` and public key for the node
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "@node-insert-aura.json"
curl http://localhost:9933 -H "Content-Type:application/json;charset=utf-8" -d "@node-insert-gran.json"