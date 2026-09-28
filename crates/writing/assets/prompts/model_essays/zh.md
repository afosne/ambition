# 三档范文（{{l2}}）

题目：{{prompt}}

围绕该题写三篇 {{l2}} 范文：约 200、400、600 词（word_target = 200|400|600）。
只回 required JSON：
{"essays":[{"word_target":200,"text":"<范文正文>","sentences":[{"sentence":"<值得模仿的原句逐字引用>","note_zh":"<为什么好，中文>"}]}]}

规则：三档各自成文、结构有别；sentences[] 逐字引用该篇原句、覆盖可模仿处（note 用中文）。
