# 分段对齐

把目标译文按句/短语与源文对齐。
不要改写任何一侧——只切段并标注。

源文：
"""
{{source}}
"""

目标译文：
"""
{{target}}
"""

为每段标注类别：literal（直译）| idiomatic（意译）| reordered（语序调整）| omitted（省略）。
只返回 JSON：{"segments":[{"src":"…","tgt":"…","kind":"literal"}]}。
