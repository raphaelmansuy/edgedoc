# edgedoc-wasm

Read, create, edit, merge, and save Word documents from Node or a browser.
The npm package name is `edgedoc-wasm`. The name `edgedoc` on npm is a different project.

```bash
make package-wasm
```

That command writes `edgedoc.js` and `edgedoc_bg.wasm` into this directory.

```javascript
const { markdownToDocument, readDocument } = require("edgedoc-wasm");

const docx = markdownToDocument("# Hello\n\n**Ada**\n");
console.log(readDocument(docx));
```

`markdownToDocumentWithImages` takes the image paths, one concatenated byte buffer,
and one length per path. The module does not read the filesystem.

Rust and Python use the same facade. The guide is the repository README.
