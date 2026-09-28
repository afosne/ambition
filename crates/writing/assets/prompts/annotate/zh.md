# 逐句批注与升级建议（{{l2}}）

评分口径：{{rubric_id}} —— 档位 {{band}}/{{max}}。

作文：
"""
{{essay}}
"""

逐句判读作文，只回 required JSON：
{"annotations":[{"sentence":"<逐字原句>","kind":"upgrade|highlight","suggested":"<更高级写法，highlight 时留空>","brief_reason":"<中文短评>","target":{"kind":"word|phrase|expression","text":"<可入卡目标>"}}]}

规则：`sentence` 必须逐字摘自原文；每句至多一条批注；平淡/中式英语句给 upgrade 建议，地道好句给 highlight；`target` 标注值得入词本的词/短语（无则省略）。
