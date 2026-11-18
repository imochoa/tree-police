/-!
# Severity

Model of `src/severity.rs`. Warm-up exercise: everything here is decided by
`cases` + `rfl`/`decide`, no induction needed.
-/

namespace TreePolice

inductive Severity where
  | log | warning | error
  deriving DecidableEq, Repr

namespace Severity

/-- Rank used for `Ord` (`#[derive(PartialOrd, Ord)]` follows variant order). -/
def rank : Severity → Nat
  | log => 0 | warning => 1 | error => 2

def asStr : Severity → String
  | log => "log" | warning => "warning" | error => "error"

/-- `FromStr`, minus the `trim().to_ascii_lowercase()` normalisation. -/
def ofStr? : String → Option Severity
  | "log" | "info" | "note" => some log
  | "warning" | "warn" => some warning
  | "error" | "err" => some error
  | _ => none

/-- `Display` then `FromStr` gets you back where you started. -/
theorem ofStr_asStr (s : Severity) : ofStr? s.asStr = some s := by
  cases s <;> rfl

/-- `rank` is injective, so the derived order is a total order. -/
theorem rank_injective {a b : Severity} (h : a.rank = b.rank) : a = b := by
  cases a <;> cases b <;> simp_all [rank]

theorem default_is_warning_between : log.rank < warning.rank ∧ warning.rank < error.rank := by
  decide

end Severity
end TreePolice
