data "aws_route53_zone" "zone" {
  name = "chainflip.xyz"
}
resource "aws_route53_record" "state-chain-x-chainflip-xyz" {
  depends_on = [hcloud_server.state-chain-node]
  count = local.state-chain-count
  name = "state-chain-${count.index}.chainflip.xyz"
  type = "A"
  ttl = "300"
  zone_id = data.aws_route53_zone.zone.id
  records = [
    hcloud_server.state-chain-node[count.index].ipv4_address
  ]
}
