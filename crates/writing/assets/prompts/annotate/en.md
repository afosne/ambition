# Sentence-level writing feedback ({{l2}})

Rubric: {{rubric_id}} — band score {{band}}/{{max}}.

Essay:
"""
{{essay}}
"""

Judge EACH sentence of the essay. Reply with the required JSON only:
{"annotations":[{"sentence":"<verbatim sentence>","kind":"upgrade|highlight","suggested":"<better wording, empty for highlight>","brief_reason":"<short reason>","target":{"kind":"word|phrase|expression","text":"<learnable item>"}}]}

Rules: `sentence` must be copied verbatim from the essay; at most one annotation per sentence; upgrade dull, wordy or Chinglish sentences with a clearly better wording; highlight genuinely idiomatic ones; `target` marks a word/phrase worth adding to the wordbook (omit if none).
