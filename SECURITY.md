# Security policy

## Supported versions

The latest release receives security fixes.

## Reporting a vulnerability

Report a vulnerability in private. Do not open a public issue.

Send a description to the maintainers through the private channel of the hosting
platform. Include the version, the input that triggers the problem, and what an
attacker gains.

You will receive an answer within seven days. If the report is valid, the fix ships
in the next release, and the report is credited unless you ask otherwise.

## Scope

EdgeDoc parses untrusted files. These properties are part of the design:

- Every parser takes a `Budget`. A part that is too large, too deep, or too wide
  returns `package.limit` or `xml.limit` before it allocates.
- The reader refuses a DTD. It does not resolve an external entity.
- The library never runs a macro, a field code, or a formula. It copies the bytes.
- `Document::save` writes a temporary sibling and renames it. It refuses a symbolic
  link at the destination.
- A document that is still encrypted returns an error. The library does not guess a
  password.

A report that defeats one of these properties is a security bug.
