# Special hyphen elements

The DOCX reader maps `w:softHyphen` and `w:noBreakHyphen` to their Unicode text characters,
as Wordcraft does. The writer restores the corresponding elements after an edit. A narrow-column
soft-hyphen fixture matches Wordcraft in the strict comparison, including its rendered image.
The [comparison before the fallback fix](hyphen-parity-result.json) records 141 passed,
187 failed and 18 blocked across 346 cases; the 343 preceding cases retained their statuses.
The [post-fallback comparison](fallback-parity-result.json) records 141 passed, 186 failed and 19
blocked. One previously failing case is blocked because the Wordcraft reference timed out.
The standard full-corpus profile does not include Source Sans 3, so the controlled profile
below supplies it explicitly to verify the fallback cases.

The selected subset font does not contain the nonbreaking-hyphen glyph. EdgeDoc now uses the
same Source Sans 3 fallback as Wordcraft for that character and excludes an unbroken soft
hyphen from painted text. Mixed-font lines also calculate the maximum of each face's ascent
plus its own line gap. With the same Source Sans 3 bytes supplied to both engines, all three
focused cases pass the [strict comparison](hyphen-fallback-parity-result.json). This is a
bounded fallback for this character; general missing-glyph fallback remains incomplete.
The [image inspection record](hyphen-image-inspection.json) identifies the matching page
images and the font and renderer inputs used to generate them.
