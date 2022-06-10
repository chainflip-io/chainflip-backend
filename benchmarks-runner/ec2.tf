module "ec2_instance" {
  source  = "terraform-aws-modules/ec2-instance/aws"
  version = "~> 3.0"

  name                        = "benchmark-github-runner"
  ami                         = "ami-09d56f8956ab235b3"
  instance_type               = lookup(local.instace_types, "${var.instance_spec}")
  key_name                    = "workloads_global_key_us-east-1"
  monitoring                  = true
  vpc_security_group_ids      = [aws_security_group.chartmuseum-ec2.id]
  subnet_id                   = data.aws_subnets.default_subnets.ids[0]
  enable_volume_tags          = true
  associate_public_ip_address = true
  user_data                   = <<-EOT
    #!/bin/bash
    sudo apt-get -y update
    cd /home/ubuntu
    mkdir actions-runner && cd actions-runner
    curl -o actions-runner-linux-x64-2.292.0.tar.gz -L https://github.com/actions/runner/releases/download/v2.292.0/actions-runner-linux-x64-2.292.0.tar.gz
    tar xzf ./actions-runner-linux-x64-2.292.0.tar.gz
    chown -R ubuntu:ubuntu /home/ubuntu/actions-runner
    export RUNNER_ALLOW_RUNASROOT=1 
    sudo --preserve-env=RUNNER_ALLOW_RUNASROOT -u ubuntu ./config.sh --url https://github.com/chainflip-io --token ${var.runner_registration_token} --labels "self-hosted,${var.runner_custom_labels}" --unattended
    ./svc.sh install ubuntu
    ./svc.sh start
  EOT
  root_block_device = [
    {
      encrypted   = true
      volume_type = "gp3"
      volume_size = var.ebs_volume_size
    },
  ]
  tags = var.tags
}
