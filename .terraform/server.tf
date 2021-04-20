locals {
  state-chain-count = 2
}
resource "hcloud_server" "state-chain-node" {
  count = local.state-chain-count
  name = "state-chain-node-${count.index}"
  server_type = "cx11"
  image = "ubuntu-20.04"
  location = "nbg1"
  ssh_keys = [
    hcloud_ssh_key.chainflip_state_chain_node_key.id
  ]
}
