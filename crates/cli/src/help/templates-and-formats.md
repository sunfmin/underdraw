Any string may hold placeholders: {path} or {path|format}. A path is dot-separated: a data root, or inside "item" also item, index and the index a repeat around it names ("index": "row"). No operators in a placeholder. {{ and }} are literal braces.

```json
"text": "{summary.remaining} left · {booking.total|money}"
```

Formats (closed set; a value that needs one and lacks it is a fault):

```
(none)       string, int
money        money             ¥18,500, per the locale
date         date              medium date
date:short   date              short date
number:N     number or int     exactly N decimals
number:N-M   number or int     N to M decimals
count        list              its length
```

Text properties — text, title, subtitle, label, detail — always get text.

Any other property given exactly one placeholder and nothing else gets the value itself, with its type: "value": "{listing.rating}", "each" items.

A bool, enum, image or object cannot be shown as text: choose words with cases. A nullable value (or one reached through it) may be shown, or given to a param that is not nullable, only where its own or an enclosing "if" or cases branch tests it != null, directly or via a condition whose top-level && chain says so. A negation guarantees nothing.
