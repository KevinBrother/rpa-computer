# Windows crossapp Stage A — SOURCE_FROZEN / STOP

## 交接

- **已停止源码修改。仅测试源码和未实现接口，不含准备功能。**
- 冻结清单：`.agents/runs/windows-crossapp-stage-a-sol-20260930/frozen-source-sha256.txt`
- 清单 SHA256：`c8e73799c1d8f940d56fa123e1320ecaa17e7b2ba82f2381f0ac9ff06e552751`
- 同目录 `frozen-source.tar.gz` 保留相对路径；共9文件：5个本轮文件、2份只读原任务及只读runner ps1/Runner.cs。
- `py_compile` **exit 0**；命令、日志、出口在同目录 `compile-command.txt`、`compile.log`、`compile-exit.txt`。
- 没有执行测试、被测API、prepare/check CLI、SSH、GUI或其他agent；没有commit/push/worktree。没有写旧fixture/分析器/oracle、root/geometry/stdio冻结源。

## 精简接口与写集

1. `acceptance-fixture/crossapp/crossapp_paths.py`：`validate_new_root(root)`、`resolve_owned_path(root, relative)`。
2. `acceptance-fixture/crossapp/crossapp_pack.py`：`prepare_owned_pack(root)`。
3. `acceptance-fixture/crossapp/crossapp_artifacts.py`：`check_notepad_bytes(actual, expected)`、`check_artifacts(root)`。
4. `tests/windows_crossapp_pack.py`：49个test方法，**AST静态计数而非运行结果**；细目见 `test-source-inventory.json`。
5. `acceptance-fixture/crossapp/API.md`：接口和测试所需最小输出字段记录。

五个API当前均只抛能力专属 `NotImplementedError`，没有先实现功能再制造RED。
测试实际调用API；调用适配器只将该异常转为带函数名/原因的unittest FAIL，不skip，不把import成功当验证。

覆盖：六例/独立nonce与trial/任务和supervisor隔离、NEWroot无覆盖、UNC/越界/重解析点保护、UTF BOM严格decode、CRLF/LF/混合换行/NFD-NFC/末尾空格区别、Explorer64文件/真实移动后的清单hash（非drag证明）、browser文件上限与不可信自报、原drag/focus任务内容和hash/安全边界、所有诊断gui_verified=false。
重解析点测试注入自有测试路径的lstat事实，无mklink/真实symlink权限依赖。仅测试自己的TemporaryDirectory允许清理；未来准备器/检查器禁止删除未知资产。

## CC Windows RED 命令（作者未执行）

在**新冻结snapshot根目录**运行；CC已有任务可直接使用其bounded runner捕获下列同等参数：

```powershell
$python = 'C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe'
& $python -B .\tests\windows_crossapp_pack.py -v
$native = $LASTEXITCODE
Write-Host "crossapp_stage_a_native_exit=$native"
exit $native
```

**预期 native exit=1**，49个测试方法均触及真实未实现能力；有subTest的用例可能报告多个失败，不要求FAIL行数恰等于49。
错误应为 `missing capability from <API>: StageA: ... missing`，不是import错误、skip、路径权限错误或超时。
unittest主要写stderr，CC须保留stdout/stderr及真实numeric exit，不混淆调度CLI terminal0。
先记录RED后停止，不运行准备CLI或GUI；Stage B等待明确授权。

## 尚未实现 / 门禁

准备器、严格安全检查实现、文件检查实现、六例生成任务/离线HTML和两个薄CLI入口均**尚未实现，属于Stage B**。
目前没有真实Windows测试结论，没有GUI授权。Stage B离线页会先读frontend-design skill。
已只读CC安装元数据；Python路径仅用于上述测试命令。安装位置/系统入口存在不证明独立安全激活；不硬编码Edge路径或包版本。任何应用必须由未来获准supervisor核对实际exe/hash、新PID+creation time+HWND及交互session。Store Notepad PID24332仍受保护，不处理它。
