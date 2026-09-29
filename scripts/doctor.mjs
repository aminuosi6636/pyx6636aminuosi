import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import path from 'node:path';

let missing = false;
function check(name, command, args) {
  try { console.log(`[OK] ${name}: ${execFileSync(command, args, { encoding: 'utf8', windowsHide: true }).trim()}`); }
  catch { console.error(`[缺少] ${name}`); missing = true; }
}
console.log(`Node.js: ${process.version}`);
check('Rust compiler', 'rustc', ['--version']);
check('Cargo', 'cargo', ['--version']);
if (process.platform === 'win32') {
  const vswhere = path.join(process.env['ProgramFiles(x86)'] ?? '', 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
  if (existsSync(vswhere)) {
    try {
      const installed = execFileSync(vswhere, ['-latest', '-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-property', 'installationPath'], { encoding: 'utf8', windowsHide: true }).trim();
      if (!installed) throw new Error('MSVC not installed');
      console.log(`[OK] MSVC: ${installed}`);
    } catch { console.error('[缺少] Microsoft C++ Build Tools / MSVC'); missing = true; }
  } else { console.error('[缺少] Microsoft C++ Build Tools / MSVC（未发现安装器）'); missing = true; }
}
if (process.platform === 'darwin') check('Xcode command line tools', 'xcode-select', ['-p']);
console.log('以上检查仅面向开发者。员工安装最终软件后不需要这些开发工具。');
process.exitCode = missing ? 1 : 0;
