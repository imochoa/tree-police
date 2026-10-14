# Variables: every variable here is typed and described unless the
# assertion below it says otherwise.

variable "region" {
  type        = string
  description = "AWS region"
}

variable "untyped" {
  description = "Mentions a type = string in prose, which doesn't count"
}
# <- @untyped_terraform_variable

variable "undescribed" {
  type = string
}
# <- @undescribed_terraform_variable

output "bucket_arn" {
  value       = aws_s3_bucket.ok.arn
  description = "ARN of the bucket"
}

output "bucket_id" {
  value = aws_s3_bucket.ok.id
}
# <- @undescribed_terraform_output

# Sensitive variables: only non-empty defaults are hardcoded secrets.

variable "db_password" {
  type        = string
  description = "Set via TF_VAR_db_password"
  default     = ""
}

variable "api_key_map" {
  type        = map(string)
  description = "Set via TF_VAR_api_key_map"
  default     = {}
}

variable "github_token" {
  type        = string
  description = "Hardcoded"
  default     = "ghp_not_a_real_token"
}
# <- @hardcoded_secret_variable

# Naming

resource "aws_s3_bucket" "ok" {
  bucket = "acme-logs"
}

resource "aws_s3_bucket" "dotted" {
  bucket = "acme-logs.example.com"
}
# <- @s3_bucket_name_contains_dots

resource "aws_ssoadmin_permission_set" "ok" {
  name         = "ReadOnly"
  instance_arn = "arn"
}

resource "aws_ssoadmin_permission_set" "long" {
  name         = "PlatformEngineeringBreakGlassAdmin"
  instance_arn = "arn"
}
# <- @sso_permission_set_name_too_long

# KMS

resource "aws_kms_key" "rotated" {
  description         = "rotated"
  enable_key_rotation = true
}

resource "aws_kms_key" "not_rotated" {
  description = "not rotated"
}
# <- @kms_key_rotation_disabled
