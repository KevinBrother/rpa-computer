# 修复真实 Windows red：跨编译测试资产不能依赖 Mac 构建绝对路径

Codex+gpt-6.1-sol 实现者；CC+GLM执行测试。工作dir本项目，无worktree/commit/push/reset，不SSH/GUI/测试执行/agent。必要cargo check允许。新的独占写集：src/mcp/remote/tls.rs 的#[cfg(test)]区、tests/remote_transport.rs、必要新 tests/support/**、本任务report .agents/reports/windows-portable-test-assets-sol-20260930.md。不要改生产TLS/Host/反馈代码，也不要碰其他owner Windows-display crate。

真实red已发生于CC编译并拷贝到Windows的lib测试exe：.agents/runs/desktop-feedback-host-windows-cc-20260930/logs/test-lib-stdout.log，237passed/2failed/6ignored，238.52s，server_config_loads_fixture和client_config_loads_fixture_ca失败；报cannot read TLS cert /Volumes/doc/workspace/datagrand/rpa/rpa-computer\\tests/fixtures/remote\\good/server.pem，目标机不存在构建机绝对路径。测试code env!(CARGO_MANIFEST_DIR)把Mac目录烘进Windows exe。这是test deployment portability缺陷，不是已证实生产TLS失败。原remote_transport也使用CARGO_BIN_EXE_computer-{host,client}构建绝对路径，CC本轮因不portable/需要mock子进程尚未执行，不要声称它red已复现。

修复要求：
- 测试用公开throwaway cert/token资产可在编译期include_bytes嵌进测试binary，然后materialize到唯一且自己拥有的temp目录；不打包到production binary。不要编译/输出任何真实部署credentials，不读全局用户目录。必要testhelper共用一个实现，cfg/test-only。
- 尽量RAII每个test/TestGuard owns temp资产，错误/正常drop清理只自己创建路径，拒绝overwrite未owned目录/文件，不以fixedpid目录潜在碰撞覆盖。API必须明确fixture缺失是harness failure而不是测试被测函数预期reject。旧server_config_rejects_mismatched_key当前可能因文件不存在也误绿，改为先保证两输入可读有效且不同，再断言真正cert/key mismatch，不弱化任何成功/拒绝要求。
- remote_transport real-binary定位支持可搬运测试bundle：明确测试环境变量override或者与testexe同目录的命名bin；同机cargo默认路径可保留作为开发fallback，但目标机若没有bin必须清晰报harness错误，不悄悄skip/执行系统PATH其它同名exe。不增加生产cli/env行为。继续强制--mock-backend，不启动native driver/真实GUI；existing timeouts/child ownership/ALPN/token/CA/name/oversize/concurrency断言都保留。
- 减少重复辅助代码；测试运行所需两个release/mock bin命名、环境变量、hash包清单等写README或report交CC。测试不再依赖远程复制整个repo或在Windows仿造/Volumes目录。
- 补regression测试源码（如资产bytes内容正确、missingbin显式失败、不同测试temp隔离、drop清理、mismatch错误不能是cannotread）；只有CC执行，不能自行green。
- 根Host源码冻结测试本轮已经生成固定exe，读取rawred即可修上述test-only源，不要覆盖首轮red日志/产物。仅compilecheck，不运行tests。完成冻结并交CC Windows定向重跑TLS模块，然后全lib/remote套件。Windows-only；Mac/Linux不测。
