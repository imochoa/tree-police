/-!
# Suppression comments

Model of `src/suppress.rs`, in two halves:

1. `parseDirective`, an executable model of `suppress::parse_directive`.
   It is deliberately written over `List Char` (not Lean's `String` API) so
   proofs are plain list induction. `Oracle.lean` exposes it to
   `tests/lean_oracle.rs`, which feeds the same random inputs to both and
   fails on any disagreement (differential testing).
2. `apply`, a model of `Suppressions::apply`, generic in what a finding and
   a directive are. Its theorems are the properties the Rust code relies on
   but never states.
-/

namespace TreePolice.Suppress

/-! ## Parsing -/

/-- Rust's `char::is_whitespace`: the Unicode `White_Space` property.
Lean's `Char.isWhitespace` only covers ASCII, so using it here would be a
model bug that the differential test catches (try it). -/
def isWs (c : Char) : Bool :=
  let n := c.toNat
  (0x9 ≤ n && n ≤ 0xD) || n == 0x20 || n == 0x85 || n == 0xA0 || n == 0x1680
    || (0x2000 ≤ n && n ≤ 0x200A) || n == 0x2028 || n == 0x2029 || n == 0x202F
    || n == 0x205F || n == 0x3000

def trimStart (s : List Char) : List Char := s.dropWhile isWs

def isBlank (s : List Char) : Bool := s.all isWs

def marker : List Char := "tree-police:".toList

/-- Everything after the first occurrence of `marker` (Rust: `text.find`). -/
def afterMarker : List Char → Option (List Char)
  | [] => none
  | c :: cs =>
    if marker.isPrefixOf (c :: cs) then some ((c :: cs).drop marker.length)
    else afterMarker cs

def isWordChar (c : Char) : Bool := c.isAlphanum || c == '-'

/-- Split on every `,` (Rust: `str::split(',')`, so `""` gives `[""]`). -/
def splitComma : List Char → List (List Char)
  | [] => [[]]
  | c :: cs =>
    match splitComma cs with
    | [] => [[]]  -- unreachable: `splitComma` is never empty
    | first :: rest => if c == ',' then [] :: first :: rest else (c :: first) :: rest

inductive Kind where
  | ignore | ignoreNextLine | ignoreFile
  deriving DecidableEq, Repr

def Kind.asStr : Kind → String
  | .ignore => "ignore" | .ignoreNextLine => "ignore-next-line" | .ignoreFile => "ignore-file"

structure RawDirective where
  kind : Kind
  selectors : Option (List (List Char))
  deriving DecidableEq, Repr

inductive DirectiveError where
  | unrecognized (word : List Char)
  | unclosedBracket
  deriving DecidableEq, Repr

def kindOf? (word : List Char) : Option Kind :=
  if word = "ignore".toList then some .ignore
  else if word = "ignore-next-line".toList then some .ignoreNextLine
  else if word = "ignore-file".toList then some .ignoreFile
  else none

def parseSelectors (rest : List Char) : Except DirectiveError (Option (List (List Char))) :=
  match rest with
  | '[' :: list =>
    match list.span (· != ']') with
    | (_, []) => .error .unclosedBracket
    | (inside, _) => .ok (some ((splitComma inside).filter (! isBlank ·)))
  | _ => .ok none

def parseDirective (text : List Char) : Option (Except DirectiveError RawDirective) := do
  let after ← afterMarker text
  let (word, rest) := (trimStart after).span isWordChar
  pure <| match kindOf? word with
    | none => .error (.unrecognized word)
    | some kind => (parseSelectors rest).map fun sels => { kind, selectors := sels }

/-! ### Properties of the parser -/

/-- No selector the parser hands to `Selector::parse` is blank — so
`Selector::parse`'s "empty rule selector" error is unreachable from
comments. -/
theorem parseSelectors_nonblank {rest : List Char} {xs : List (List Char)}
    (h : parseSelectors rest = .ok (some xs)) : ∀ x ∈ xs, isBlank x = false := by
  unfold parseSelectors at h
  split at h
  · split at h
    · cases h
    · cases h; intro x hx; simpa using (List.mem_filter.mp hx).2
  · cases h

/-- Text without the marker is never a directive. -/
theorem parseDirective_none_of_afterMarker {text : List Char}
    (h : afterMarker text = none) : parseDirective text = none := by
  simp [parseDirective, h]

/-- The finding behind `tree-police: ignore [x]`: one space before the
bracket and the selector list is silently dropped, so the comment
suppresses *every* rule on that line instead of just `x`. -/
example : parseDirective "# tree-police: ignore [x]".toList
    = some (.ok { kind := .ignore, selectors := none }) := by
  rfl

example : parseDirective "# tree-police: ignore[x]".toList
    = some (.ok { kind := .ignore, selectors := some ["x".toList] }) := by
  rfl

/-! ## Applying directives -/

section Apply
variable {F D : Type} (covers : D → F → Bool)

/-- `Suppressions::apply`: keep findings no directive covers. -/
def apply (ds : List D) (fs : List F) : List F :=
  fs.filter fun f => !ds.any (covers · f)

/-- Suppression only removes findings, and keeps the rest in order. -/
theorem apply_sublist (ds : List D) (fs : List F) : (apply covers ds fs).Sublist fs :=
  List.filter_sublist

/-- No directives, no change (the Rust early return is just an optimisation). -/
theorem apply_nil (fs : List F) : apply covers [] fs = fs := by
  simp [apply]

/-- Applying twice is the same as applying once. -/
theorem apply_idem (ds : List D) (fs : List F) :
    apply covers ds (apply covers ds fs) = apply covers ds fs := by
  simp [apply, List.filter_filter]

/-- More directives never let a finding back in. -/
theorem apply_antitone {ds ds' : List D} (hsub : ∀ d ∈ ds, d ∈ ds') (fs : List F) :
    ∀ f ∈ apply covers ds' fs, f ∈ apply covers ds fs := by
  intro f hf
  simp only [apply, List.mem_filter, Bool.not_eq_true', List.any_eq_false] at hf ⊢
  exact ⟨hf.1, fun d hd => hf.2 d (hsub d hd)⟩

/-- Directives can be applied one comment at a time, in any grouping. -/
theorem apply_append (ds₁ ds₂ : List D) (fs : List F) :
    apply covers (ds₁ ++ ds₂) fs = apply covers ds₂ (apply covers ds₁ fs) := by
  simp [apply, List.filter_filter, Bool.and_comm]

end Apply
end TreePolice.Suppress
