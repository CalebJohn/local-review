# Syntax-Aware Diffing

- **Sub-group analysis in mixed hunks:** Re-diff canonical forms within a change group to classify individual lines, rather than marking the whole group as semantic.
- **Import reorder detection:** Identify import blocks, sort canonically, compare.
- **Per-language quote equivalence:** Only normalize quotes in languages where `"` and `'` are interchangeable for the same construct.

