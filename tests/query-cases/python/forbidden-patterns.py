# Query-case fixtures for the "forbidden" category rules in
# queries/rules-py.scm (one `.scm` file holds every Python rule, grouped
# by `(#set! category ...)` -- see queries/README.md).
#
# Run with `just query-test` (wired into `cargo test` via
# tests/query_cases.rs). An assertion comment names the capture expected to
# fire on the line directly above it. An unannotated line is an *implicit*
# "nothing should fire here" assertion: the harness fails if any capture
# fires on a line with no matching annotation. So each rule's positive and
# negative example sit side by side below -- no separate "clean" section.
#
# Every fixture is checked against *all* rules, not just its own category,
# so a negative example here must also avoid tripping a security/temporal
# rule by accident (see security-patterns.py and temporal-patterns.py for
# cases where that happened).

print("debug")
# <- @forbidden_print
logger.info("debug")

# TODO: fix this
# <- @todo
# TODO #123: fix this

subprocess.run(["ls"])
# <- @subprocess_no_timeout
subprocess.run(["ls"], timeout=30)

try:
    do_thing()
except:
    pass
# <- @bare_except

os.system("rm -rf /tmp/cache")
# <- @os_system_usage
os.system  # referenced, not called

[[j for j in i] for i in range(10)]
# <- @nested_listcomp
[j for j in range(10)]


foo(1, 2, 3, 4)  # Matches (4 positional arguments)
# <- @too_many_posargs
foo(x + 1, y, z, w)  # Matches (4 positional arguments, complex expressions)
# <- @too_many_posargs
foo(1, 2, 3)  # Excluded (3 positional arguments)

