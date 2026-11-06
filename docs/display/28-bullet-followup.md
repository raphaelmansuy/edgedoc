# Bullet mapping and geometry follow-up

The Symbol, Wingdings, Wingdings 2/3 and Webdings Unicode mapping is derived from Wordcraft under
its MIT licence; the source includes the licence and attribution. Numbering levels using those
fonts now map their stored symbol characters for display. List labels share the first text line's
baseline. A left-aligned bullet with a tab suffix and a fitting hanging indent places its marker
at the hanging position and its text at the left indent; other arrangements retain their existing
layout path.

The [339-case comparison](bullet-parity-result.json) reports 136 passed, 185 failed and 18
blocked. One previously failing external case passed, and one formerly blocked case rendered but
failed. The new Symbol-bullet fixture is a failure: Wordcraft uses Symbol font bytes outside the
controlled profile and the label's font selection and metrics are not yet equivalent.

The [image record](bullet-image-inspection.json) shows the fifth page of the docx4j guide. The
inspected bullet markers and text now align horizontally, but page contents differ because the
engines paginate differently. This is not a document-level match.
