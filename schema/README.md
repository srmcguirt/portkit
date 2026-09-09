# portkit-schema — schema grounding

Stops an agent inventing a column, and stops it reading a stale one.

## Why a snapshot

fellwork-data's `schema/` holds 143 `CREATE TABLE` and 239 `ALTER TABLE` across
87 files, including 16 `DROP COLUMN`. **No single file states the current
shape** — only `pg_catalog` does. An agent that reads `0001_baseline.sql`
doesn't hallucinate; it reports stale truth, which is worse because it looks
sourced.

But agents often can't reach the live database: no credentials in CI, no
touching production, offline work. So introspect once, commit a fingerprinted
snapshot, and check against that — the same bargain the parity harness makes
with fixtures. A snapshot diff is a schema change, and reviews like one.

## Verified against real DDL

fellwork-data's migrations loaded into Postgres 17, then introspected:

```
$ pks pull "postgres://..." snapshot.json source work pub usr ops lexgraph
  tables       47
  columns      522
  fingerprint  4d19c1cb9acf6e21
  snapshot     73478 B
```

```
$ pks check snapshot.json source.tokens surface_from
NO  `surface_from` does not exist
    did you mean `surface_form`? (distance 2)
    [fellwork @ PostgresCatalog, captured 2026-09-09T17:42:14, fingerprint 4d19c1cb9acf6e21]

$ pks check snapshot.json source.token
NO  `source.token` does not exist
    did you mean `source.tokens`? (distance 1)

$ pks check snapshot.json source.tokens user_email
NO  `user_email` does not exist
    no close match
```

That last one matters as much as the others: **a bad suggestion is worse than
none**, because the model will take it.

## Provenance is mandatory

Every answer is stamped with source, kind, capture time, and fingerprint. A
snapshot answer and a live answer can disagree, and the caller has to know
which it got — a wrong invariant is worse than a missing one.

`SourceKind::is_authoritative()` draws the line: `PostgresCatalog` states fact,
`SqlMigrations` states intent. A checker that conflates them will confidently
approve a column that was dropped.

Locators never carry credentials; provenance travels into agent context and
into commits. There is a test for that.

## Provenance of the code

`src/pg/` is vendored from `magna-introspect` (github.com/fellwork/magna),
commit `8519d710`, 2026-04-26 — same author, same MIT OR Apache-2.0 terms.
Copied rather than depended on because magna is a `publish = false` workspace
in another repo. Its declared `magna-types` dependency was unused and dropped.
Re-sync by diffing that path and updating the commit in `src/pg/mod.rs`.

## Status

Prototype. Working: Postgres capture, snapshot + fingerprint, table/column
resolution with suggestions, offline tests against committed real DDL.

Not built yet: `x-schema-ref` wiring so the registry checks arguments before
dispatch; drift detection against live; OpenAPI/GraphQL/TypeScript sources.
