"""验证许可筛选边界，不执行下载或改变发布包：允许非 GPL 分支，拒绝必须承担 GPL 的组合。"""
import ast
from pathlib import Path

source = Path('tools/collect-licenses.py').read_text(encoding='utf-8')
module = ast.parse(source)
function = next(node for node in module.body if isinstance(node, ast.FunctionDef) and node.name == 'select_license')
namespace = {}
exec(compile(ast.Module(body=[function], type_ignores=[]), '<license-policy>', 'exec'), namespace)
select = namespace['select_license']
assert select('Apache-2.0 OR GPL-2.0-only') == ['Apache-2.0']
assert select('(MIT OR Apache-2.0) AND BSD-3-Clause') == ['MIT', 'BSD-3-Clause']
assert select('MIT OR Apache-2.0 OR LGPL-2.1-or-later') == ['MIT']
for expression in ['GPL-3.0-only', 'AGPL-3.0-only', 'MIT AND GPL-2.0-only', 'LicenseRef-Unknown', 'MIT WITH LicenseRef-Unknown', '']:
    try:
        select(expression)
    except RuntimeError:
        continue
    raise AssertionError(expression)
print('许可分支及拒绝边界通过。')