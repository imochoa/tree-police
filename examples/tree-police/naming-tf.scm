; Example repo-local rules: copy this directory to `.tree-police/` in your
; repo (or point --rules-dir at it) and swap the placeholder conventions for
; your own. These used to ship as built-in rules; an org-specific prefix like
; `acme-` fires on every real bucket, so they're opt-in now.
;
; Try them: tree-police --rules-dir examples/tree-police tests/fixtures/terraform

; S3 buckets -- illustrates enforcing an org-wide resource-naming prefix.
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
  (#set! category "org-naming")
  (#set! description "S3 bucket name must start with acme- (example org prefix)")
) @s3_bucket_missing_prefix

; IAM Roles -- illustrates enforcing a resource-type suffix convention.

(block
  (identifier) @_block_type (#eq? @_block_type "resource")
  (string_lit (template_literal) @_resource_type (#eq? @_resource_type "aws_iam_role"))
  (body
    (attribute
      (identifier) @_attr (#eq? @_attr "name")
      (expression (literal_value (string_lit
        (template_literal) @_role_name (#not-match? @_role_name "-role$"))))))
  (#set! category "org-naming")
  (#set! description "IAM role name must end with -role")
) @iam_role_missing_role_suffix
