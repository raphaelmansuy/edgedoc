# OPC packages, flavors, namespaces, and encryption.

> Part of the EdgeDoc specification. Date: 2026-11-04. Requirement ids are normative. See [the index](README.md).

## 8. Package layer

### 8.1 Package source

```rust
pub trait PackageSource {
    fn read_part(&mut self, name: &PartName) -> Result<Vec<u8>, PackageError>;
    fn part_names(&self) -> Result<Vec<PartName>, PackageError>;
}
```

`ED-P-04`. `ZipSource` and `MemorySource` both implement `PackageSource`. Tests use `MemorySource`. WebAssembly uses `MemorySource` over the caller's bytes.

`ED-P-01`. Part names compare case-insensitively, per ECMA-376 Part 2. Two names that differ only by case are one part. WordCraft documents this rule in `docs/file-format-parity.md`.

`ED-P-02`. The reader finds the main part from the office-document relationship. The relationship type is the string WordCraft names `rt::OFFICE_DOC` in `crates/docx/src/package.rs`.

`ED-P-03`. A missing `[Content_Types].xml`, a missing main part, or a ZIP that is not a package returns `Error::Package`. It does not return an empty document.

### 8.2 Flavors

`ED-P-05`. The flavor selects the main content type. The four strings are the ones in WordCraft `Flavor::main_content_type` (`crates/docx/src/lib.rs`).

`ED-P-06`. A macro flavor may contain `vbaProject.bin`. The library stores that part as a passthrough part. The library never executes it.

**WARNING:** A `.docm` or `.dotm` package can contain macros. EdgeDoc preserves the bytes. EdgeDoc does not run them. A caller that sends the saved file to Word can still run the macros there.

### 8.3 Conformance of markup

`ED-P-07`. The reader accepts Transitional and Strict namespaces. It maps Strict names onto the Transitional local names before the catalog match. WordCraft does this in `crates/docx/src/xml.rs`.

`ED-P-08`. Version 1 writes Transitional markup. `WriteOptions::conformance` may be `Strict` only after a later version claims that write path. Until then, `Strict` returns `Error::Unsupported`.

`ED-P-09`. `mc:AlternateContent` keeps the branch the catalog can read. The other branches stay opaque markup in order. A later save writes the choice and the fallback.

### 8.4 Encryption

`ED-P-10`. With the `crypt` feature, an agile-encrypted package and a standard-encrypted package open when the password is correct. A missing password returns `package.password_required`. A wrong password returns `package.wrong_password`. These are the WordCraft errors `PasswordRequired` and `WrongPassword`.

`ED-P-11`. Without the `crypt` feature, an encrypted package returns `package.encrypted_unsupported`. The library does not return a partial document.
