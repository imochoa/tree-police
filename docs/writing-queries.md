# Writing tree-sitter queries (`.scm` files)

A quick reference for writing the S-expression queries that go in
`queries/*.scm` (embedded rules) or `.tree-police/*.scm` (repo-local rules —
see [`docs/pre-commit-example.md`](pre-commit-example.md)). Based on the
[official docs](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/)
and the [live playground](https://tree-sitter.github.io/tree-sitter/7-playground.html),
where you can paste code on the left and a query on the right and see matches
highlighted live — the fastest way to iterate on a query before dropping it
into a `.scm` file.

**Why `.scm`?** It's short for *Scheme* (the Lisp dialect) — tree-sitter
queries aren't Scheme code, but they're the same parenthesized S-expression
syntax, so editors already know how to syntax-highlight them reasonably well
under that extension. Nothing here is executed by a Scheme interpreter.

## Basics

A pattern is an S-expression naming a node type, optionally with children:

```scheme
(binary_expression (number_literal) (number_literal))
```

Children can be omitted — this matches any `binary_expression` with *at least
one* `string_literal` child:

```scheme
(binary_expression (string_literal))
```

Only named nodes get parens. Anonymous nodes (punctuation, keywords) are
matched as quoted strings:

```scheme
(binary_expression operator: "!=" right: (null))
```

## Fields

Prefix a child with `field_name:` to constrain *which* child it must be, not
just that it exists somewhere:

```scheme
(assignment_expression
  left: (member_expression
    object: (call_expression)))
```

Negate a field with `!` to match nodes that *lack* it:

```scheme
(class_declaration
  name: (identifier) @class_name
  !type_parameters)
```

## Captures

Append `@name` to tag a node so you can refer to it (and so tree-police
actually reports it — a query needs at least one capture to produce a
finding):

```scheme
(class_declaration
  name: (identifier) @the-class-name
  body: (class_body
    (method_definition
      name: (property_identifier) @the-method-name)))
```

A capture name starting with `_` is matched but never reported — the
convention this project uses for helper captures that only exist to drive a
predicate. See [`queries/README.md`](../queries/README.md#rule-conventions)
for the full rule-authoring conventions (exactly one reportable capture per
pattern, `(#set! severity "…")`, `(#set! category "…")`,
`(#set! description "…")`).

## Wildcards

`_` matches any node; `(_)` matches any *named* node:

```scheme
(call (_) @call.inner)
```

## Quantifiers

Postfix operators, same meaning as regex:

| Op | Meaning |
| --- | --- |
| `*` | zero or more |
| `+` | one or more |
| `?` | zero or one |

```scheme
(class_declaration
  (decorator)* @the-decorator
  name: (identifier) @the-name)

(call_expression
  function: (identifier) @the-function
  arguments: (arguments (string)? @the-string-arg))
```

## Grouping

Parens around a *sequence* of siblings (no leading node type) group them, and
groups can be quantified too:

```scheme
(
  (comment)
  ("," (number))*
)
```

## Alternation

`[...]` matches any one of several alternatives — like a regex character
class:

```scheme
(call_expression
  function: [
    (identifier) @function
    (member_expression property: (property_identifier) @method)
  ])

["break" "else" "for" "if" "return" "while"] @keyword
```

## Anchors

`.` constrains adjacency. Placement matters:

- **Leading** — child must be the parent's *first* named child:

  ```scheme
  (array . (identifier) @first-element)
  ```

- **Trailing** — child must be the parent's *last* named child:

  ```scheme
  (block (_) @last-expression .)
  ```

- **Between two patterns** — the two matches must be immediate siblings:

  ```scheme
  (dotted_name (identifier) @prev . (identifier) @next)
  ```

Anchors interact with quantifiers (a `*`/`+`/`?` that matches zero nodes
doesn't block the anchor) but can't sit at the edge of a group `(...)` or
alternation `[...]`, since those aren't nodes themselves.

## Predicates

Extra `(#predicate? ...)` clauses filter matches after structural matching.
Common ones:

```scheme
((identifier) @variable.builtin
  (#eq? @variable.builtin "self"))

((identifier) @constant
  (#match? @constant "^[A-Z][A-Z_]+$"))

((identifier) @variable.builtin
  (#any-of? @variable.builtin "arguments" "module" "console" "window"))
```

`#eq?`/`#not-eq?` compare a capture to a string or another capture;
`#match?`/`#not-match?` use regex; `#any-of?`/`#not-any-of?` check against a
list. tree-police's embedded matcher evaluates this standard set
(`#eq?`/`#not-eq?`/`#match?`/`#not-match?`/`#any-of?`) — **non-core
predicates like `#is?`/`#not-has-ancestor?` are silently treated as
unsatisfied and drop the whole match**, not just the predicate; see the
Gotchas in [`AGENTS.md`](../AGENTS.md#gotchas).

## Special nodes

```scheme
(ERROR) @error-node          ; parser couldn't make sense of this
(MISSING) @missing-node      ; parser inserted a token to recover
(MISSING ";") @missing-semi  ; a specific missing node type
```

## Putting it together (a tree-police rule)

Match calls to `os.system(...)`, reporting the whole call without leaking the
helper captures used to identify it:

```scheme
(call
  function: (attribute
    object: (identifier) @_mod (#eq? @_mod "os")
    attribute: (identifier) @_func (#eq? @_func "system")
  )
  (#set! severity "error")
  (#set! category "forbidden")
  (#set! description "os.system() invokes a shell; prefer subprocess")
) @os_system_usage
```

(This is the real `os_system_usage` rule in
[`queries/rules-py.scm`](../queries/rules-py.scm).)

## Tips for writing new queries

1. **Look at the real tree before guessing node names.** Options, roughly
   least to most setup:
   - Paste your snippet into the [playground](https://tree-sitter.github.io/tree-sitter/7-playground.html)
     (select the matching language) — shows the live tree and highlights
     matches as you edit the query, no local install needed.
   - In Neovim (0.9+), `:InspectTree` opens a live syntax-tree split synced
     to your cursor; `:Inspect` shows the highlight groups/captures for the
     node under the cursor. See
     [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter).
   - The official `tree-sitter` CLI's `tree-sitter parse <file>` prints the
     tree directly, if you have a grammar checked out locally.
2. Start broad (one node type, one capture), run it, then narrow with
   fields/anchors/predicates until it's precise.
3. For a one-off filter you don't want surfaced (a helper node that only
   exists to drive a predicate), name its capture starting with `_`.

## References

- [Tree-sitter query syntax](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/)
- [Tree-sitter playground](https://tree-sitter.github.io/tree-sitter/7-playground.html)
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) /
  [neovim's own example queries](https://github.com/neovim/neovim/tree/master/runtime/queries)
- [`queries/README.md`](../queries/README.md) — this project's naming
  convention and rule-authoring rules
- [`AGENTS.md`](../AGENTS.md#gotchas) — query-authoring gotchas specific to
  tree-police's embedded matcher
