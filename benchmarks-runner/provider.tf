provider "aws" {
  region = var.region
}
provider "aws" {
  region = "eu-central-1"
  alias = "eu-central-1"
}

terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 3.0"
    }
  }
}
