```json
"phases": { "loading": { "id": "loading", "component": "Screen", "children": [
              { "id": "sk", "component": "Skeleton", "shape": ["image 4:3", "line 60%"] } ] } }
```

A phase tree has no data roots: no paths, no conditions on data. Inside root, every non-nullable path has a value. A section that loads on its own later is a nullable field and a condition, not a phase.
