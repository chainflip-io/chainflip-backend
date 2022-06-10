variable "region" {
  type        = string
  description = "The AWS region to deploy to"
  default     = "us-east-1"
}
variable "tags" {
  type        = map(string)
  description = "Tags to apply to the resources"
  default = {
    project     = "benchmarks-runner"
    environment = "production"
    target      = "external"
    terraform   = "true"
  }
}
variable "ebs_volume_size" {
  description = "The size of the EBS volume to create"
  type        = number
  default     = 8
}
variable "instance_spec" {
  description = "The type of instance to create"
  type        = string
}
variable "runner_registration_token" {
  description = "The registration token for the runner"
  type        = string
}
variable "runner_custom_labels" {
  description = "The custom labels for the runner"
  type        = string
}
