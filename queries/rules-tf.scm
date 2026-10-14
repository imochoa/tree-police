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

; S3 bucket names must be lowercase (AWS rejects uppercase).

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

; S3 bucket names with dots break virtual-hosted-style HTTPS (the wildcard
; certificate doesn't cover the extra labels) and S3 Transfer Acceleration.

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_s3_bucket"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "bucket")
      (expression (literal_value (string_lit
        (template_literal) @_bucket_name (#match? @_bucket_name "\\."))))))
  (#set! category "naming")
  (#set! description "S3 bucket name contains dots, which break virtual-hosted-style HTTPS")
) @s3_bucket_name_contains_dots

; IAM Identity Center permission set names are capped at 32 characters by AWS;
; longer ones only fail at apply time.

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_ssoadmin_permission_set"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "name")
      (expression (literal_value (string_lit
        (template_literal) @_pset_name (#match? @_pset_name "^.{33,}$"))))))
  (#set! category "naming")
  (#set! description "SSO permission set name exceeds the 32-character AWS limit")
) @sso_permission_set_name_too_long

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

; Variables and outputs without type/description. A text check on the block
; (there's no "missing child" syntax), anchored to a line start so prose in a
; description that mentions "type =" doesn't count.

((block
  (identifier) @_block_type (#eq? @_block_type "variable")) @untyped_terraform_variable
  (#not-match? @untyped_terraform_variable "(?m)^\\s*type\\s*=")
  (#set! category "conventions")
  (#set! description "Variable has no type"))

((block
  (identifier) @_block_type (#eq? @_block_type "variable")) @undescribed_terraform_variable
  (#not-match? @undescribed_terraform_variable "(?m)^\\s*description\\s*=")
  (#set! category "conventions")
  (#set! description "Variable has no description"))

((block
  (identifier) @_block_type (#eq? @_block_type "output")) @undescribed_terraform_output
  (#not-match? @undescribed_terraform_output "(?m)^\\s*description\\s*=")
  (#set! category "conventions")
  (#set! description "Output has no description"))

; aws_kms_key leaves automatic rotation off unless enable_key_rotation = true.

((block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_kms_key"))) @kms_key_rotation_disabled
  (#not-match? @kms_key_rotation_disabled "(?m)^\\s*enable_key_rotation\\s*=\\s*true")
  (#set! category "conventions")
  (#set! description "aws_kms_key without enable_key_rotation = true"))

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

; Variable with "password", "secret", "token" or "api_key" in the name and a
; non-empty default value. Empty defaults ("", {}, [], null) are the usual
; "set it via TF_VAR_*" placeholder, not a hardcoded secret.
(block
  (identifier) @_block_type (#eq? @_block_type "variable")
  (string_lit
    (template_literal) @_var_name (#match? @_var_name "(?i)password|secret|token|api_key"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "default")
      (expression) @_value (#not-match? @_value "^(\"\"|\\{\\s*\\}|\\[\\s*\\]|null)$")))
  (#set! severity "error")
  (#set! category "security")
  (#set! description "Sensitive variable with a hardcoded default value")
) @hardcoded_secret_variable
