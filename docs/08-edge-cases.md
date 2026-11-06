# Required behavior for hostile and boundary input.

> Part of the EdgeDoc specification. Date: 2026-11-04. Requirement ids are normative. See [the index](README.md).

## 15. Edge cases

Each row is a requirement. The test name is the CI name. "Manual" means a Word check outside CI.

| Id | Case | Required behavior | Test |
|---|---|---|---|
| `ED-X-01` | Empty template | One empty paragraph. Word opens it | `empty_document_has_one_paragraph` |
| `ED-X-02` | No office-document relationship | `package.not_word` | `missing_main_part` |
| `ED-X-03` | Part names differ only by case | One part | `part_names_are_case_insensitive` |
| `ED-X-04` | ZIP bomb, huge declared size | `package.limit` before allocation past the budget | `zip_budget` |
| `ED-X-05` | XML deeper than 256 | `xml.limit` | `xml_depth` |
| `ED-X-06` | DTD and external entity | Rejected. The entity is not fetched | `no_dtd` |
| `ED-X-07` | Strict namespace | Read succeeds. Write is Transitional | `strict_read` |
| `ED-X-08` | `mc:AlternateContent` | Supported branch is modeled. Other branches remain opaque | `alternate_content` |
| `ED-X-09` | Encrypted, no password | `package.password_required` | `password_required` |
| `ED-X-10` | Wrong password | No document body. A verifier returns `package.wrong_password`. Until phase 9, a password returns `package.encrypted_unsupported` | `wrong_password` |
| `ED-X-11` | Macro package | VBA bytes survive a save. No macro runs | `vba_passthrough` |
| `ED-X-12` | Unedited save | Output bytes equal input bytes | `clean_save_is_identical` |
| `ED-X-13` | Unknown part | Bytes survive an edit in another part | `unknown_part_kept` |
| `ED-X-14` | Edit inside tracked changes | `model.needs_drop_consent` until the caller allows the drop | `revision_consent` |
| `ED-X-15` | Undefined style | Behaves as Normal. The name is saved | `undefined_style` |
| `ED-X-16` | Heading without `outlineLvl` | Outline level is the heading level | `builtin_heading_level` |
| `ED-X-17` | 64th table column | Clamped to 63 | `table_63_columns` |
| `ED-X-18` | Nested table past depth 24 | `model.limit` | `table_depth` |
| `ED-X-19` | Offset inside a character | `model.bad_offset` | `char_boundary` |
| `ED-X-20` | Unbalanced field markers | `balance_fields` removes orphans before save. The report counts them | `balance_fields` |
| `ED-X-21` | `MERGEFIELDX` | Not a merge field | `mergefield_boundary` |
| `ED-X-22` | Merge field with a space in the name | Quoted instruction. Lookup uses the unquoted name | `quoted_field_name` |
| `ED-X-23` | Header missing, policy `Error` | Whole batch fails. No result document is returned | `missing_field_errors` |
| `ED-X-24` | Header missing, policy `Empty` | The value is empty | `missing_field_empty` |
| `ED-X-25` | Duplicate headers | `merge.duplicate_header` | `duplicate_header` |
| `ED-X-26` | `7` compared with `7.0` | The condition holds | `numeric_compare` |
| `ED-X-27` | `NEXT` then a field | The later field uses the next record | `next_record` |
| `ED-X-28` | `SKIPIF` holds | That copy is absent. Later records still merge | `skipif` |
| `ED-X-29` | Merge field in the header | Each letter is its own section | `header_field_sections` |
| `ED-X-30` | Directory kind | No page break between records | `directory_join` |
| `ED-X-31` | Zero records | `merge.no_records` | `empty_range` |
| `ED-X-32` | Preview | Template `plain_text` is unchanged | `preview_does_not_mutate` |
| `ED-X-33` | CSV quoted comma and newline | One record, two cells | `csv_rfc4180` |
| `ED-X-34` | CSV formula-like text `=1+1` | The cell value is the text. It is not evaluated | `csv_is_text` |
| `ED-X-35` | xlsx formula | Cached value only | `sheet_cached_value` |
| `ED-X-36` | Save onto a symlink | `io.destination`. The link target is unchanged | `save_refuses_symlink` |
| `ED-X-37` | Disk full during save | Destination unchanged | `save_keeps_destination` |
| `ED-X-38` | Right-to-left run | `w:rtl` and the bidi language round-trip | `rtl_run` |
| `ED-X-39` | Flat OPC package | `read_bytes` accepts it when the root is the package root | `flat_opc` |
| `ED-X-40` | Word 2003 XML | Version 1 returns `unsupported` | `wordml2003_rejected` |
| `ED-X-41` | Picture is EMF or WMF | Opaque drawing. PNG, JPEG, and GIF are owned | `metafile_opaque` |
| `ED-X-42` | Content control lock | A locked control rejects replacement with `model.locked` | `locked_control` |
| `ED-X-43` | Password in a record cell | The error and the log do not add that cell unless the caller asked to debug records | `no_secret_in_error` |
| `ED-X-44` | Joined result past 200,000 blocks | `merge.limit`. No partial document | `merge_block_budget` |
| `ED-X-45` | Python and WASM merge | Same records produce the same package bytes as Rust | `adapter_byte_parity` |
| `ED-X-46` | Markdown round trip | Exported Markdown reads back to the same text. A missing image returns `markdown.image_missing`. A path with `..` returns `markdown.image_dir` | `markdown_reads_back_into_a_document` |

`ED-X-42` adds the code `model.locked` to the error table in [07-adapters.md](07-adapters.md).

`ED-S-08`. Property tests generate a builder document, write it, read it, and compare the owned model. They do not compare passthrough bytes, because a created document has none.
