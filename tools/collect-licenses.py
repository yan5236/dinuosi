"""收集当前 Windows 实际运行依赖的许可及弱 copyleft 源码，拒绝只允许 GPL/AGPL 的包。"""
import concurrent.futures
import hashlib
import json
import re
import shutil
import subprocess
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORK = ROOT / '.local/distribution'
WORK.mkdir(parents=True, exist_ok=True)


def windows_packages():
    """从锁定 Cargo 图计算 Windows 桌面程序的普通依赖闭包，排除仅供开发/构建的包。
    返回第三方包元数据列表与 Cargo 输入哈希审计；只写项目缓存中的 Windows 包清单。
    本地依赖必须已下载；Cargo、文件读取或依赖图异常向上抛出，不复用过期清单。
    """
    result = subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
                             '--filter-platform', 'x86_64-pc-windows-msvc'], cwd=ROOT,
                            check=True, capture_output=True, text=True, encoding='utf-8')
    metadata = json.loads(result.stdout)
    packages = {item['id']: item for item in metadata['packages']}
    nodes = {item['id']: item for item in metadata['resolve']['nodes']}
    pending = [item['id'] for item in metadata['packages'] if item['name'] == 'dinuosi-desktop']
    visited = set()
    while pending:
        identifier = pending.pop()
        if identifier in visited:
            continue
        visited.add(identifier)
        pending.extend(dependency['pkg'] for dependency in nodes[identifier]['deps']
                       if any(kind['kind'] is None for kind in dependency['dep_kinds']))
    if not visited:
        raise RuntimeError('找不到桌面程序的锁定依赖图')
    selected = [packages[key] for key in sorted(visited) if not packages[key]['name'].startswith('dinuosi-')]
    (WORK / 'windows-packages.json').write_text(json.dumps(selected, indent=2), encoding='utf-8')
    inputs = [ROOT / 'Cargo.lock', ROOT / 'Cargo.toml'] + [Path(item['manifest_path']) for item in metadata['packages']
                                     if item['id'] in metadata['workspace_members']]
    audit = {'inputs': [{'path': str(path.relative_to(ROOT)),
                         'sha256': hashlib.sha256(path.read_bytes()).hexdigest()} for path in inputs]}
    return selected, audit


PACKAGES, AUDIT = windows_packages()
OUT = WORK / 'licenses'
CACHE = WORK / 'license-cache'
OUT.mkdir(exist_ok=True)
CACHE.mkdir(exist_ok=True)


def upstream_license(package):
    """按包元数据中的仓库和发布提交查询 GitHub 许可原文，返回缓存路径与来源 URL 的列表。
    没有仓库或未找到原文时返回空列表；写入许可缓存，网络失败跳过该候选，文件/元数据异常向上抛出。
    不生成版权文本；内容哈希由调用方收集时计算，不作为此处的下载校验。
    """
    directory = Path(package['manifest_path']).parent
    info_path = directory / '.cargo_vcs_info.json'
    repository = package.get('repository') or ''
    if package['name'] == 'zune-inflate':
        repository = 'https://github.com/etemesi254/zune-image'
    match = re.match(r'https://github.com/([^/]+/[^/]+)', repository)
    if not match:
        return []
    project = match[1].removesuffix('.git')
    info = json.loads(info_path.read_text()) if info_path.exists() else {}
    commit = info.get('git', {}).get('sha1') or ('v' + package['version'])
    candidates = ['LICENSE', 'LICENSE-MIT', 'LICENSE-APACHE', 'LICENSE.MIT', 'LICENSE.APACHE', 'LICENSE.txt', 'LICENSE.md', 'COPYING', 'UNLICENSE', 'LICENCE', 'LICENSE-MPL-2.0', 'LICENSE-APACHE-2.0', 'LICENSE-CC0', 'LICENSE.CC0']
    results = []
    for name in candidates:
        destination = CACHE / (package['name'] + '-' + package['version'] + '-' + name)
        url = f'https://raw.githubusercontent.com/{project}/{commit}/{name}'
        if destination.exists():
            results.append((destination, url))
            continue
        try:
            with urllib.request.urlopen(url, timeout=12) as response:
                content = response.read()
            if b'<html' in content.lower():
                continue
            destination.write_bytes(content)
            results.append((destination, url))
        except (urllib.error.URLError, TimeoutError):
            pass
        # 对单许可证只取一个原始文本；双许可证收集所有可用原文。
        if results and ' OR ' not in package['license'] and '/' not in package['license']:
            break
    return results


def declared_notice(package, chosen):
    """发布包及对应提交没有完整许可文件时，保留真实声明和完整发布源码，附所引用 SPDX 标准文本。
    package 为发布包元数据，chosen 为已选择的许可 ID；返回声明文件路径与来源说明列表。
    复制完整发布源码，按需下载标准许可文本并写缓存；下载或文件错误向上抛出。
    不补造版权信息；这里只生成声明材料，缺口标记由 collect 写入依赖清单。
    """
    directory = Path(package['manifest_path']).parent
    declaration = directory / 'Cargo.toml.orig'
    if not declaration.exists():
        declaration = directory / 'Cargo.toml'
    target = OUT / 'sources' / f'{package["name"]}-{package["version"]}'
    shutil.copytree(directory, target, dirs_exist_ok=True)
    destination = CACHE / f'{package["name"]}-{package["version"]}-DECLARED-LICENSE.txt'
    content = 'Upstream published license declaration, not an invented copyright notice.\n'
    content += f'Package: {package["name"]} {package["version"]}\n'
    content += f'Authors recorded by upstream: {package.get("authors", [])}\n'
    content += f'License identifier recorded by upstream: {package["license"]}\n'
    content += 'No standalone complete license file was present in this published crate or its inspected source commit.\n'
    content += 'The complete published source and original Cargo manifest accompany this notice in sources/.\n\n'
    content += declaration.read_text(encoding='utf-8') + '\n'
    for selected in chosen:
        standard = CACHE / f'SPDX-{selected}.txt'
        if not standard.exists():
            with urllib.request.urlopen(f'https://spdx.org/licenses/{selected}.txt', timeout=30) as response:
                standard.write_bytes(response.read())
        content += '\nReferenced standard license text (placeholders retained without alteration):\n'
        content += standard.read_text(encoding='utf-8')
    destination.write_text(content, encoding='utf-8')
    return [(destination, 'original published Cargo license declaration + corresponding source + SPDX standard text')]

def select_license(expression):
    """为当前锁定依赖使用的平面 AND/OR、旧式 / 及外层括号表达式选择允许的许可 ID 列表。
    expression 为许可声明字符串；没有允许分支时抛出 RuntimeError，无文件或网络副作用。
    不支持任意嵌套的 SPDX 表达式；不能据此宣称完成法律审查，也不猜测替代授权。
    """
    approved = {'MIT','Apache-2.0','BSD-2-Clause','BSD-3-Clause','ISC','Zlib','0BSD','CC0-1.0','Unicode-3.0','MPL-2.0','BSL-1.0','NCSA','Unlicense','MIT-0'}
    groups = expression.replace('/', ' OR ').split(' AND ')
    chosen = []
    for group in groups:
        options = group.strip('() ').split(' OR ')
        available = [x.strip('() ') for x in options if ' WITH ' not in x]
        selection = next((x for x in available if x in approved), None)
        if selection is None:
            raise RuntimeError(f'不允许的许可证：{expression}')
        chosen.append(selection)
    return chosen


def collect(package):
    """读取 package 元数据并返回许可、原始声明和文件哈希记录；本项目包直接返回 None。
    写入许可材料，MPL 包附完整发布源码；五个已记录的上游缺口生成声明并标记，不进入阻断清单。
    未允许的许可或未审查的原文缺失直接抛错，下载和文件异常向上抛出，阻止生成新的审计结果。
    """
    if package['name'].startswith('dinuosi-'):
        return None
    expression = package.get('license') or ''
    chosen = select_license(expression)
    directory = Path(package['manifest_path']).parent
    files = [p for p in directory.iterdir() if p.is_file() and re.match(r'(?i)^(license|licence|copying|notice|copyright|unlicense)', p.name)]
    originals = [(p, 'published crate') for p in files] or upstream_license(package)
    item = {'name':package['name'], 'version':package['version'], 'declared':expression, 'selected':' AND '.join(chosen), 'repository':package.get('repository'), 'notices':[]}
    if not originals:
        reviewed_omissions = {('convert_case', '0.4.0'), ('hexf-parse', '0.2.1'), ('mac', '0.1.1'),
                              ('seahash', '4.1.0'), ('simd_helpers', '0.1.0')}
        if (package['name'], package['version']) not in reviewed_omissions:
            raise RuntimeError(f'缺少未经审查的许可原文：{package["name"]} {package["version"]}')
        originals = declared_notice(package, chosen)
        item['license_file_omitted_upstream'] = True
    for path, source in originals:
        destination = OUT / f'{package["name"]}-{package["version"]}-{path.name}'
        shutil.copyfile(path, destination)
        item['notices'].append({'file':destination.name,'source':source,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
    if 'MPL-2.0' in chosen:
        target = OUT / 'sources' / f'{package["name"]}-{package["version"]}'
        shutil.copytree(directory, target, dirs_exist_ok=True)
    return item


with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
    results = [item for item in executor.map(collect, PACKAGES) if item]
results.sort(key=lambda item: (item['name'], item['version']))
(OUT / 'rust-dependencies.json').write_text(json.dumps(results, indent=2, ensure_ascii=False), encoding='utf-8')
blocked = [item for item in results if 'blocked' in item]
(WORK / 'license-blockers.json').write_text(json.dumps(blocked, indent=2, ensure_ascii=False), encoding='utf-8')
print(f'Windows runtime dependencies: {len(results)}; missing original notices: {len(blocked)}', flush=True)
for item in blocked:
    print(item['name'], item['version'], item['blocked'], flush=True)
# 全部依赖成功收集后才提交审计标记，失败不能让旧清单匹配新的 Cargo 输入。
AUDIT['inventory_sha256'] = hashlib.sha256((OUT / 'rust-dependencies.json').read_bytes()).hexdigest()
(WORK / 'license-audit.json').write_text(json.dumps(AUDIT, indent=2), encoding='utf-8')
