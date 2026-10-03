; Python rules, grouped by `category` rather than by file (see
; queries/README.md). Conventions:
;   - The one capture without a leading `_` is the rule ID (reported).
;   - Helper captures used only in predicates are prefixed with `_`.
;   - Severity is declared per pattern: (#set! severity "error|warning|log").
;     Omitting it defaults to "warning".

; ============================================================
; category: forbidden
; ============================================================

; Direct print() usage (enforce logger instead)
(call
  function: (identifier) @_func (#eq? @_func "print")
  (#set! severity "warning")
  (#set! category "forbidden")
  (#set! description "Use the logger instead of print()")
) @forbidden_print

; TODO/FIXME without a ticket reference
((comment) @todo
  (#match? @todo "(?i)TODO|FIXME")
  (#not-match? @todo "#[0-9]+")
  (#set! severity "log")
  (#set! category "forbidden")
  (#set! description "TODO/FIXME without a ticket reference (e.g. #123)"))

; subprocess.run/call/check_output/check_call without a timeout argument
(call
  function: (attribute
    object: (identifier) @_mod (#eq? @_mod "subprocess")
    attribute: (identifier) @_method (#match? @_method "^(run|call|check_output|check_call)$")
  )
  arguments: (argument_list) @_args
  (#not-match? @_args "timeout")
  (#set! severity "warning")
  (#set! category "forbidden")
  (#set! description "subprocess call without a timeout")
) @subprocess_no_timeout

; Bare except: clause (swallows everything, including KeyboardInterrupt)
((except_clause) @bare_except
  (#set! severity "warning")
  (#set! category "forbidden")
  (#set! description "Bare except swallows all exceptions"))

; os.system usage (invokes a shell; prefer subprocess)
(call
  function: (attribute
    object: (identifier) @_mod (#eq? @_mod "os")
    attribute: (identifier) @_func (#eq? @_func "system")
  )
  (#set! severity "error")
  (#set! category "forbidden")
  (#set! description "os.system() invokes a shell; prefer subprocess")
) @os_system_usage

; Nested list comprehensions are hard to understand
(list_comprehension
  (list_comprehension)
  (#set! severity "warning")
  (#set! category "forbidden")
)@nested_listcomp

; Function calls: Match if there are MORE THAN 3 positional arguments (any type)
(call
  arguments: (argument_list) @_args
  (#match? @_args "^(\\(([^,=]+,){3,}[^,=]+\\))")  ; Matches 4+ positional args
  (#set! severity "error")
  (#set! category "forbidden")
  (#set! description "Too many positional arguments")
)@too_many_posargs

; ============================================================
; category: security (all severity "error")
; ============================================================

; Hardcoded credentials or API keys
(assignment
  left: (identifier) @_var_name
  right: (string) @_value
  (#match? @_var_name "(?i)password|secret|token|api_key|apikey|api_secret|credential")
  (#match? @_value ".{8,}")
  (#set! severity "error")
  (#set! category "security")
  (#set! description "Possible hardcoded credential")
) @hardcoded_credential

; SQL query built by string concatenation (SQL injection risk)
(assignment
  right: (binary_operator
    left: (string) @_sql_string
    operator: "+"
    (#match? @_sql_string "(?i)SELECT|INSERT|UPDATE|DELETE|CREATE|DROP|ALTER")
  )
  (#set! severity "error")
  (#set! category "security")
  (#set! description "SQL built by string concatenation (injection risk)")
) @sql_concat

; pickle.loads usage (arbitrary code execution risk)
(call
  function: (attribute
    object: (identifier) @_mod (#eq? @_mod "pickle")
    attribute: (identifier) @_func (#eq? @_func "loads")
  )
  (#set! severity "error")
  (#set! category "security")
  (#set! description "pickle.loads can execute arbitrary code")
) @pickle_loads_usage

; eval() or exec() usage
(call
  function: (identifier) @_func
  (#match? @_func "^(eval|exec)$")
  (#set! severity "error")
  (#set! category "security")
  (#set! description "eval()/exec() executes arbitrary code")
) @eval_exec_usage

; os.popen usage (shell injection risk)
(call
  function: (attribute
    object: (identifier) @_mod (#eq? @_mod "os")
    attribute: (identifier) @_func (#eq? @_func "popen")
  )
  (#set! severity "error")
  (#set! category "security")
  (#set! description "os.popen invokes a shell (injection risk)")
) @os_popen_usage

; ============================================================
; category: temporal (severity "warning")
; ============================================================

; Function that references temporal but isn't @activity/@workflow decorated
(decorated_definition
  (decorator
    (identifier) @_decorator_name
    (#not-match? @_decorator_name "^(activity|workflow)$")
  )
  (function_definition
    name: (identifier) @_func_name
    body: (block) @_body
    (#match? @_body "temporal")
  )
  (#set! severity "warning")
  (#set! category "temporal")
  (#set! description "Temporal function missing @activity/@workflow decorator")
) @missing_temporal_decorator

; await call directly in a *_workflow function's top-level body (does not
; descend into nested try/if/for blocks -- see below).
;
; Two things confirmed by testing (see tests/query_cases.rs), neither
; documented anywhere upstream:
;
; 1. The `[ ... ]` alternation below exists because the two engines parse a
;    bare `await foo()` statement differently: the `tree-sitter-python`
;    *crate* (tree-police's embedded grammar) wraps it in `expression_statement`;
;    the Nix-provided grammar behind the `tree-sitter query` CLI does not.
;    Same language, different grammar builds, different AST shape.
; 2. The original pattern also had `(#not-has-ancestor? @_call
;    try_statement)` to exclude awaits wrapped in try/except. That predicate
;    was dead code: `body: (block (await ...))` only matches a DIRECT child
;    of the function's top-level block, and an await inside a try's nested
;    block can never be a direct child of the outer one -- so the predicate
;    could never have excluded anything the structural pattern didn't
;    already exclude. Removed rather than kept as misleading decoration;
;    non-core predicates like it are a separate hazard anyway, since the
;    embedded Rust matcher treats an unrecognized predicate as unsatisfied
;    and drops the whole match rather than ignoring it.
(function_definition
  name: (identifier) @_func_name
  body: (block
    [
      (expression_statement (await (call) @_call))
      (await (call) @_call)
    ]
  )
  (#match? @_func_name ".*_workflow$")
  (#set! severity "warning")
  (#set! category "temporal")
  (#set! description "Unhandled await in workflow (wrap in try/except)")
) @unhandled_await_in_workflow

; ============================================================
; category: web
; ============================================================

; Flask route handler missing a pydantic @validate(...) decorator.
;
; Structural "this decorator is absent" can't be expressed directly in a
; tree-sitter query (no negation over sibling nodes; see the
; `#not-has-ancestor?` gotcha in AGENTS.md for why a predicate-based
; workaround isn't viable under tree-police either). Same trick as
; missing_temporal_decorator: capture the whole decorated_definition and
; `#not-match?` its full text for "@validate(" instead of structurally
; checking for a missing sibling decorator.
;
; The outer `(...)` wrapping is load-bearing, not style: a predicate can
; only reference a capture that's already been declared *earlier in the
; same text*, and a node's own capture is only declared at its closing
; paren -- so `@missing_route_validation` (captured on the whole
; decorated_definition) can't be referenced by a predicate that's one of
; decorated_definition's own children (textually before that closing
; paren). Wrapping `(decorated_definition ...) @missing_route_validation`
; plus the predicate in one more pair of parens makes the capture and the
; predicate siblings at the *outer* level instead, where ordering works.
; Confirmed by testing -- `tree-sitter query` and `tree-police` both reject the
; unwrapped form with "Invalid capture name".
;
; Caveat: this scans the *entire* decorated_definition, including the
; function body -- a route handler that happens to contain the literal
; text "@validate(" in a comment or string would be a false negative. Also
; doesn't enforce decorator *order* (validate directly after route vs.
; anywhere among the decorators); "present at all" is what's checked.
((decorated_definition
  (decorator
    (call
      function: (attribute
        attribute: (identifier) @_route_attr (#eq? @_route_attr "route")
      )
    )
  )
) @missing_route_validation
(#not-match? @missing_route_validation "@validate\\(")
(#set! severity "error")
(#set! category "web")
(#set! description "Flask route handlers must have @validate(...) directly after the route decorator"))

; The rest of this category is a small illustrative set of Python/web style
; rules -- not an exhaustive policy. Add your own via a repo-local
; `.tree-police/` directory (see README) or by extending this file.

; Use an f-string instead of str.format().
(call
  function: (attribute
    attribute: (identifier) @_attr (#eq? @_attr "format")
  )
  (#set! severity "warning")
  (#set! category "web")
  (#set! description "Use an f-string instead of .format()")
) @str_format_call

; Nested ternary (conditional_expression inside another conditional_expression)
; -- both the explicitly-parenthesized form and the unparenthesized chained
; form (`a if b else c if d else e`, which the grammar nests in the `else`
; branch the same way) are covered. Use an explicit if/else instead.
(conditional_expression
  [
    (conditional_expression)
    (parenthesized_expression (conditional_expression))
  ]
  (#set! severity "warning")
  (#set! category "web")
  (#set! description "Avoid nested ternary operators; use an explicit if/else")
) @nested_ternary
