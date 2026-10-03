; Terraform/HCL rules, grouped by `category` rather than by file (see
; queries/README.md). A small illustrative set -- not an exhaustive policy.
; Add your own via a repo-local `.tree-police/` directory (see README) or by
; extending this file.
;
; Conventions: helper captures are prefixed with `_`; the trailing capture
; is the rule ID; severity defaults to "warning" unless set.
;
; Only literal string values are checked; interpolated expressions
; (e.g. "${var.name_prefix}-service") require runtime validation.
; Regex uses Rust regex crate (no lookaheads); use #not-match? instead.

; ============================================================
; category: naming
; ============================================================

; S3 Buckets -- illustrates enforcing an org-wide resource-naming prefix.
; "acme-" is a placeholder; swap it (or add more rules like it) for your
; own convention.

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_s3_bucket"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "bucket")
      (expression (literal_value (string_lit
        (template_literal) @_bucket_name (#not-match? @_bucket_name "^acme-"))))))
  (#set! category "naming")
  (#set! description "S3 bucket name must start with acme- (example org prefix)")
) @s3_bucket_missing_prefix

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_s3_bucket"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "bucket")
      (expression (literal_value (string_lit
        (template_literal) @_bucket_name (#match? @_bucket_name "[A-Z]"))))))
  (#set! category "naming")
  (#set! description "S3 bucket name must be all lowercase")
) @s3_bucket_name_uppercase

; IAM Roles -- illustrates enforcing a resource-type suffix convention.

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_iam_role"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "name")
      (expression (literal_value (string_lit
        (template_literal) @_role_name (#not-match? @_role_name "-role$"))))))
  (#set! category "naming")
  (#set! description "IAM role name must end with -role")
) @iam_role_missing_role_suffix

; ============================================================
; category: conventions
; ============================================================

; null_resource usage -- prefer terraform_data (TF 1.4+)
(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "null_resource"))
  (#set! category "conventions")
  (#set! description "Prefer terraform_data over null_resource")
) @null_resource_usage

; Inline security group rules -- prefer standalone aws_vpc_security_group_{ingress,egress}_rule

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_security_group"))
  (body
    (block
      (identifier) @_nested (#eq? @_nested "ingress")))
  (#set! category "conventions")
  (#set! description "Use aws_vpc_security_group_ingress_rule instead of an inline ingress block")
) @inline_sg_ingress_rule

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_security_group"))
  (body
    (block
      (identifier) @_nested (#eq? @_nested "egress")))
  (#set! category "conventions")
  (#set! description "Use aws_vpc_security_group_egress_rule instead of an inline egress block")
) @inline_sg_egress_rule

; Hardcoded credentials in provider blocks -- credentials come from AWS_PROFILE or OIDC

(block
  (identifier) @_block_type (#eq? @_block_type "provider")
  (body
    (attribute
      (identifier) @_attr (#match? @_attr "^(access_key|secret_key)$")))
  (#set! category "conventions")
  (#set! description "Provider credentials must come from AWS_PROFILE or OIDC, not hardcoded")
) @hardcoded_provider_credentials

(block
  (identifier) @_block_type (#eq? @_block_type "provider")
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "profile")))
  (#set! category "conventions")
  (#set! description "Use the AWS_PROFILE env var instead of a hardcoded provider profile")
) @hardcoded_provider_profile

; Provisioner anti-patterns -- prefer declarative Terraform over imperative glue

(block
  (identifier) @_prov_type (#eq? @_prov_type "provisioner")
  (string_lit (template_literal) @_prov_kind (#eq? @_prov_kind "local-exec"))
  (#set! category "conventions")
  (#set! description "local-exec provisioner is imperative glue; prefer declarative Terraform")
) @local_exec_provisioner

(block
  (identifier) @_prov_type (#eq? @_prov_type "provisioner")
  (string_lit (template_literal) @_prov_kind (#eq? @_prov_kind "remote-exec"))
  (#set! category "conventions")
  (#set! description "remote-exec provisioner is imperative glue; prefer declarative Terraform")
) @remote_exec_provisioner

; Legacy S3 bucket configuration anti-patterns (AWS provider ~> 4.0+ split
; these into dedicated resources)

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_s3_bucket"))
  (body
    (block
      (identifier) @_nested (#eq? @_nested "lifecycle_rule")))
  (#set! category "conventions")
  (#set! description "Use aws_s3_bucket_lifecycle_configuration instead of an inline lifecycle_rule block")
) @s3_bucket_inline_lifecycle_rule

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_s3_bucket"))
  (body
    (block
      (identifier) @_nested (#eq? @_nested "server_side_encryption_configuration")))
  (#set! category "conventions")
  (#set! description "Use aws_s3_bucket_server_side_encryption_configuration instead of an inline block")
) @s3_bucket_inline_encryption

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_s3_bucket"))
  (body
    (block
      (identifier) @_nested (#eq? @_nested "versioning")))
  (#set! category "conventions")
  (#set! description "Use aws_s3_bucket_versioning instead of an inline versioning block")
) @s3_bucket_inline_versioning

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_s3_bucket"))
  (body
    (block
      (identifier) @_nested (#eq? @_nested "logging")))
  (#set! category "conventions")
  (#set! description "Use aws_s3_bucket_logging instead of an inline logging block")
) @s3_bucket_inline_logging

; ============================================================
; category: security (all severity "error")
; ============================================================

; S3 bucket with public ACL
(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#match? @_resource_type "aws_s3_bucket"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "acl")
      (expression (literal_value (string_lit
        (template_literal) @_acl_value (#match? @_acl_value "(?i)public"))))))
  (#set! severity "error")
  (#set! category "security")
  (#set! description "S3 bucket with a public ACL")
) @public_s3_bucket

; Security group rule open to 0.0.0.0/0
(attribute
  (identifier) @_attr (#eq? @_attr "cidr_blocks")
  (expression
    (collection_value
      (tuple
        (expression (literal_value (string_lit
          (template_literal) @_cidr (#eq? @_cidr "0.0.0.0/0")))))))
  (#set! severity "error")
  (#set! category "security")
  (#set! description "CIDR block open to the entire internet (0.0.0.0/0)")
) @open_cidr_block

; IAM policy with wildcard actions
(attribute
  (identifier) @_attr (#eq? @_attr "actions")
  (expression
    (collection_value
      (tuple
        (expression (literal_value (string_lit
          (template_literal) @_action (#eq? @_action "*")))))))
  (#set! severity "error")
  (#set! category "security")
  (#set! description "IAM policy grants wildcard actions (*)")
) @wildcard_iam_action

; IAM policy with wildcard resources
(attribute
  (identifier) @_attr (#eq? @_attr "resources")
  (expression
    (collection_value
      (tuple
        (expression (literal_value (string_lit
          (template_literal) @_res (#eq? @_res "*")))))))
  (#set! severity "error")
  (#set! category "security")
  (#set! description "IAM policy grants wildcard resources (*)")
) @wildcard_iam_resource

; Variable with "password", "secret", or "token" in the name that has a default value
(block
  (identifier) @_block_type (#eq? @_block_type "variable")
  (string_lit
    (template_literal) @_var_name (#match? @_var_name "(?i)password|secret|token|api_key"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "default")))
  (#set! severity "error")
  (#set! category "security")
  (#set! description "Sensitive variable with a hardcoded default value")
) @hardcoded_secret_variable
