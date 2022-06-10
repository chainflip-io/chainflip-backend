terraform {
  backend "s3" {
    bucket         = "chainflip-terraform-workloads-state"
    dynamodb_table = "chainflip-terraform-workloads-lock"
    key            = "benchmark-github-runners/terraform.tfstate"
    region         = "eu-central-1"
    encrypt        = true
  }
}
