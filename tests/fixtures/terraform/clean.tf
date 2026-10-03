# Compliant Terraform -- no violations expected

resource "aws_s3_bucket" "state" {
  bucket = "acme-terraform-state-eu-central-1-123456789012-dev"
}

resource "aws_s3_bucket_versioning" "state" {
  bucket = aws_s3_bucket.state.id
  versioning_configuration {
    status = "Enabled"
  }
}

resource "aws_iam_role" "eks_node" {
  name               = "dev-eks-node-role"
  assume_role_policy = "{}"
}

resource "aws_security_group" "web" {
  name = "dev-web-app-sg"
}

resource "aws_vpc_security_group_ingress_rule" "https" {
  security_group_id = aws_security_group.web.id
  from_port         = 443
  to_port           = 443
  ip_protocol       = "tcp"
  cidr_ipv4         = "10.0.0.0/8"
}

resource "terraform_data" "trigger" {
  input = timestamp()
}
