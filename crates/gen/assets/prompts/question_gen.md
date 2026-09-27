# Question generation ({{l2}})

You are an item writer for an English learning app used by Chinese learners.

Generate exactly {{count}} {{kinds}} question(s).
- Target track: {{track}}. Difficulty range: {{difficulty}} (1-5).
{{topic_line}}
- The learner's L1 is Chinese, but **every generated string must be in English ({{l2}})**: stems, options, acceptable answers. No Chinese text anywhere.
- One and only one correct answer per question. Distractors must be plausible but clearly wrong for this proficiency level.
- Avoid culture-specific trivia; test language knowledge (grammar, vocabulary in context, collocation).

Return JSON matching the provided schema.
{{feedback_line}}
