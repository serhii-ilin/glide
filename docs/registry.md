# Requirement and feature registry

Glide keeps one file per requirement and per feature. The file name is the ID: a short
kebab-case **slug**. Choose a slug, never a number, so parallel branches and agents cannot
collide on a counter or on a shared table line.

```
docs/requirements/<slug>.md   one requirement
docs/features/<slug>.md       one feature (description, links, tests, manual checks)
docs/milestones.md            milestone overview (hand-written, included in the generated page)
tests/features/<slug>.feature the feature's Gherkin acceptance spec (same slug)
docs/features.md              GENERATED tables, git-ignored
docs/manual-tests.md          GENERATED manual-check table, git-ignored
```

`docs/features.md` and `docs/manual-tests.md` are built from the files above and are not
committed (committed tables would conflict on every merge). Build and check them with:

```sh
node scripts/registry.mjs            # generate the two tables
node scripts/registry.mjs --check    # validate; exit 1 on errors (CI runs this)
node scripts/registry-test.mjs       # negative-case tests for the validator
```

## File format

A file is front matter followed by a Markdown body. Front matter lines are `key: value`,
where the value is JSON (a quoted string or a list). A list may continue on the following
indented lines after a bare `key:`.

Feature (`docs/features/<slug>.md`):

| Field | Meaning |
| --- | --- |
| `slug` | Equals the file name, kebab-case, unique among features |
| `kind` | `feature` |
| `title` | Short name |
| `status` | `planned`, `in-progress`, `done`, `partial`, `deferred` (a reason may follow), `removed` |
| `aliases` | Legacy IDs such as `["F-085"]`; unique across the whole registry |
| `requirements` | Requirement slugs this feature satisfies (may be empty) |
| `gherkin` | `tests/features/<slug>.feature`, or `backfill pending` until the script is written |
| `tests` | List of test names, written `module::path::fn_name` or `module::{a,b}`; prose entries such as `manual <slug>#1` are allowed |
| `mcp` | `glide_*` tool names, `planned <slug> (tool)`, or `n/a (reason)` |
| `manual` | List of `{steps, expected, aliases}` for checks that need a real window or OS input; its ID is `<slug>#<n>` |

The body is the user-visible description (one or two sentences), then an optional
`## Notes` section.

Requirement (`docs/requirements/<slug>.md`): `slug`, `kind: "requirement"`, `title`, `status`
(`active` or `removed`), `milestone`, `aliases`, `features` (feature slugs) and `source`
(the standard, benchmark or product decision that justifies it). The body is the testable
statement. Links must resolve both ways: a feature lists its requirements and each of those
requirements lists the feature back.

## Adding a feature or requirement

1. Choose a slug (`reddit-compat`, `mcp-scroll`). Never use a number.
2. Copy `tests/features/_TEMPLATE.feature` to `tests/features/<slug>.feature` and write the
   scenarios first.
3. Create `docs/features/<slug>.md` (and `docs/requirements/<slug>.md` when it adds a new
   requirement; link both directions).
4. Run `node scripts/registry.mjs --check`, then write the failing test, then the code.

## Legacy IDs

Every row of the old sequential registry (`R-xxx`, `F-xxx`, `M-xxx`) is kept in `aliases`.
Find an old ID with `grep -rl '"F-085"' docs/features docs/requirements`; the generated
tables show the alias next to each slug. Source comments that still mention an old ID stay
valid through its alias. Manual checks moved into the `manual:` list of the feature that owns
them (the first feature when the old row named several).
