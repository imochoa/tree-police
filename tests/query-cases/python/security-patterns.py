# Query-case fixtures for the "security" category rules in
# queries/rules-py.scm. Run with `just query-test`. See
# forbidden-patterns.py for the format.
#
# All rules in rules.scm apply to every fixture, not just this category's --
# negative examples here are chosen to avoid *also* tripping an unrelated
# category's rule (e.g. the eval_exec_usage negative below uses logger.info,
# not print(), since print() would trigger @forbidden_print too).

password = "supersecretvalue123"
# <- @hardcoded_credential
password = get_secret("password")                  # not a literal string

query = "SELECT * FROM users WHERE id=" + user_id
# <- @sql_concat
query = f"SELECT * FROM users WHERE id={user_id}"  # f-string, not concatenation

pickle.loads(data)
# <- @pickle_loads_usage
json.loads(data)                                   # json, not pickle

eval("1 + 2")
# <- @eval_exec_usage
logger.info("eval this carefully")                  # not an eval() call

os.popen("ls")
# <- @os_popen_usage
subprocess.run(["ls"], timeout=30)                  # not os.popen
