# Segment alignment

Align the reference translation to the source sentence by segments.
Do NOT rewrite either side — only segment and label.

Source:
"""
{{source}}
"""

Target:
"""
{{target}}
"""

Label each segment kind: literal | idiomatic | reordered | omitted.
Reply with the required JSON only: {"segments":[{"src":"...","tgt":"...","kind":"literal"}]}.
