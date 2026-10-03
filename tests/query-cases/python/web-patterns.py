# Query-case fixtures for the "web" category rules in
# queries/rules-py.scm. Run with `just query-test`. See
# forbidden-patterns.py for the format and the "all rules apply to every
# fixture" caveat.

@items_api.route("", methods=["POST"])
def add_item(body: AddItemBody) -> NewItemResponse:
    pass
# <- @missing_route_validation

@items_api.route("", methods=["POST"])
@validate(response_by_alias=True, on_success_status=201)
def add_item_validated(body: AddItemBody) -> NewItemResponse:
    pass

greeting = "Hello, {}".format(name)
# <- @str_format_call
greeting = f"Hello, {name}"

category = a if cond else (b if cond2 else c)
# <- @nested_ternary
if cond:
    category = a
else:
    category = b
