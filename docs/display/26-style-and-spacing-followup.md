# Character-style and spacing follow-up

This is a historical comparison against the clean local Wordcraft commit
`1c7b5013fab765f204cf435b9f8814b46b7c1278`, not Microsoft Word.

The reader now retains explicit off values for run properties, so a child style or direct run
can turn off inherited bold, italic, strike and related effects. `w:u w:val="none"` also turns
off inherited underline. The writer preserves explicit off values after an edit. Automatic
paragraph-spacing flags are imported, cascaded and saved; the body layout uses Wordcraft's
14-point automatic spacing and its first-paragraph/list exceptions. Ordinary hyphens keep their
Unicode break opportunities, with Wordcraft's additional digit and slash cases.

The [335-case report](style-spacing-parity-result.json) contains 134 passed, 182 failed and 19
blocked. Two files that were killed during the parallel rendering pass were rerun individually;
the report records the retries, and both remain failures. Two previously failing external cases
passed. The two focused style/spacing fixtures passed, and no earlier pass regressed.

The [image record](style-spacing-image-inspection.json) shows the declaration's upright Contents
heading and aligned body spacing. At that stage the footer page number still differed because its
paragraph frame was unsupported. Later frame and hard-break work has further changed the output.
