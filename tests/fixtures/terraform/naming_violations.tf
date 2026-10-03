# Intentional naming convention violations for testing

# S3: missing acme- prefix
resource "aws_s3_bucket" "legacy_backup" {
  bucket = "example-terraform"
}

# S3: uppercase letters
resource "aws_s3_bucket" "upper" {
  bucket = "MyBucket-Data-Export"
}

# IAM role: missing -role suffix
resource "aws_iam_role" "no_suffix" {
  name               = "dev-eks-node"
  assume_role_policy = "{}"
}

# IAM role: uppercase (also missing -role suffix, but that's a separate rule)
resource "aws_iam_role" "upper_role" {
  name               = "Dev-EKS-Node-Role"
  assume_role_policy = "{}"
}
