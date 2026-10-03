# Intentional Terraform convention violations for testing

# Anti-pattern: null_resource (prefer terraform_data)
resource "null_resource" "provisioner" {
  provisioner "local-exec" {
    command = "echo hello"
  }
}

# Anti-pattern: inline ingress/egress rules on security group
resource "aws_security_group" "inline_rules" {
  name = "dev-test-sg"

  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["10.0.0.0/8"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

# Anti-pattern: hardcoded credentials in provider
provider "aws" {
  access_key = "AKIAIOSFODNN7EXAMPLE"
  secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
  region     = "eu-central-1"
}

# Anti-pattern: hardcoded profile in provider
provider "aws" {
  profile = "my-profile"
  region  = "eu-central-1"
}

# Anti-pattern: local-exec provisioner (imperative glue)
resource "terraform_data" "deploy" {
  provisioner "local-exec" {
    command = "deploy.sh"
  }
}

# Anti-pattern: inline S3 bucket configuration blocks (use separate resources)
resource "aws_s3_bucket" "legacy_style" {
  bucket = "acme-legacy-example-eu-central-1-123456789012-dev"

  versioning {
    enabled = true
  }

  logging {
    target_bucket = "acme-logs-eu-central-1-123456789012-dev"
  }

  lifecycle_rule {
    enabled = true
    transition {
      days          = 30
      storage_class = "STANDARD_IA"
    }
  }

  server_side_encryption_configuration {
    rule {
      apply_server_side_encryption_by_default {
        sse_algorithm = "aws:kms"
      }
    }
  }
}
