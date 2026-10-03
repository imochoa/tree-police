# Query-case fixtures for the "temporal" category rules in
# queries/rules-py.scm. Run with `just query-test`. See
# forbidden-patterns.py for the format and for the "all rules apply to
# every fixture" caveat.

@retry
def send_notification():
    temporal_client.signal("notify")
# <- @missing_temporal_decorator

@activity
def send_notification_properly():
    temporal_client.signal("notify")

async def run_workflow():
    await do_thing()
# <- @unhandled_await_in_workflow

async def run_workflow_safely():
    try:
        await do_thing()
    except Exception:
        pass
# <- @bare_except
# (a "forbidden" category rule; it matches ANY except clause, not just a
# truly bare `except:` -- a known looseness, not fixed here.)
