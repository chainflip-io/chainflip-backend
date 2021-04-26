provider "aws" {
  region = "eu-central-1"
}
provider "hcloud" {
  token = var.hcloud_token
}

terraform {
  backend "s3" {
    bucket = "terraform-state-chainflip-state-chain"
    dynamodb_table = "terraform-lock-chainflip-state-chain"
    region = "eu-central-1"
    key = "terraform.tfstate"
    encrypt = true
  }
}
