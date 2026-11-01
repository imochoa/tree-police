import Lean.Data.Json
import TreePolice.Suppress

open Lean (Json)
open TreePolice.Suppress

/-- Same JSON shape `tests/lean_oracle.rs` builds from the Rust result. -/
def encode : Option (Except DirectiveError RawDirective) → Json
  | none => .null
  | some (.error (.unrecognized w)) =>
    Json.mkObj [("error", "unrecognized"), ("word", Json.str (String.ofList w))]
  | some (.error .unclosedBracket) => Json.mkObj [("error", "unclosed")]
  | some (.ok d) =>
    Json.mkObj [
      ("kind", Json.str d.kind.asStr),
      ("selectors", match d.selectors with
        | none => .null
        | some xs => Json.arr (xs.map (Json.str ∘ String.ofList)).toArray)]

partial def loop (stdin : IO.FS.Stream) (stdout : IO.FS.Stream) : IO Unit := do
  let line ← stdin.getLine
  if line.isEmpty then return
  let out ← match Json.parse line >>= Json.getStr? with
    | .ok text => pure (encode (parseDirective text.toList)).compress
    | .error e => throw (IO.userError s!"oracle: bad input line: {e}")
  stdout.putStrLn out
  stdout.flush
  loop stdin stdout

def main : IO Unit := do
  loop (← IO.getStdin) (← IO.getStdout)
