---
name: bad-skill
---

# bad-skill

This skill intentionally omits the `description` frontmatter field so that
validation reports an issue and `validation.ok` is `false`. Used to exercise
the `--validation-failed` filter on the `list` subcommand.
