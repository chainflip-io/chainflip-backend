resource "aws_s3_bucket" "terraform-state-chainflip-state-chain" {
  bucket = "terraform-state-chainflip-state-chain"
  acl = "private"
  versioning {
    enabled = true
  }

  lifecycle {
    prevent_destroy = true
  }

  tags = {
    Owner = "https://github.com/chainflip-io/chainflip-backend.git"
    Name = "state-chain"
  }
}

resource "aws_s3_bucket_public_access_block" "chainflip-drone-ci-cache" {
  bucket = aws_s3_bucket.terraform-state-chainflip-state-chain.id

  block_public_acls   = true
  block_public_policy = true
  restrict_public_buckets = true
  ignore_public_acls = true
}

resource "aws_dynamodb_table" "terraform-lock-chainflip-state-chain" {
  name = "terraform-lock-chainflip-state-chain"
  hash_key = "LockID"
  read_capacity = 20
  write_capacity = 20
  billing_mode = "PAY_PER_REQUEST"

  attribute {
    name = "LockID"
    type = "S"
  }

  tags = {
    Owner = "https://github.com/chainflip-io/chainflip-backend.git"
    Name = "state-chain"
  }
}
