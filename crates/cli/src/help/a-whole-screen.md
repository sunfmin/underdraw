It passes `check`. A condition holds no format and no arithmetic, so a list's length is a derived int beside it.

```json
{
  "screen": "Inbox", "title": "Inbox", "tokens": "tokens.json",
  "data":  { "inbox": "Inbox" },
  "types": { "Inbox": { "count": { "type": "int 0..", "derived": { "app": "messages' length" } },
                        "messages": "Message[]" },
             "Message": { "from": "string 1..", "unread": "bool = false" } },
  "conditions": { "hasMessages": { "when": "inbox.count > 0", "label": "Has mail" } },
  "fixtures": {
    "some":  { "inbox": { "count": 1, "messages": [{ "from": "Ana", "unread": true }] } },
    "empty": { "inbox": { "count": 0, "messages": [] } }
  },
  "root": { "id": "inbox", "component": "Screen", "padding": "space.4", "children": [
    { "id": "empty", "component": "EmptyState", "if": "!hasMessages",
      "icon": "inbox", "title": "No mail" },
    { "id": "list", "component": "List", "if": "hasMessages", "each": "inbox.messages",
      "item": { "id": "msg-{index}", "component": "ListItem", "title": "{item.from}",
                "leadingIcon": { "cases": [ { "if": "item.unread", "then": "mail.fill" },
                                            { "else": "drafts" } ] },
                "on": { "tap": { "navigate": "Message", "with": {"index": "{index}"} } } } } ] },
  "phases": { "loading": { "id": "loading", "component": "Screen", "children": [
    { "id": "sk", "component": "Skeleton", "shape": ["line 90%", "line 90%"] } ] } }
}
```
