resource "tls_private_key" "chainflip_state_chain_node_key" {
  algorithm = "RSA"
  rsa_bits = 4096
}

resource "hcloud_ssh_key" "chainflip_state_chain_node_key" {
  name = "client_key"
  public_key = tls_private_key.chainflip_state_chain_node_key.public_key_openssh
}

resource "local_file" "chainflip_state_chain_node_key_pem" {
  filename = "/Users/tomburton/.ssh/chainflip_state_chain_node_key_pem"
  content = tls_private_key.chainflip_state_chain_node_key.private_key_pem
}
