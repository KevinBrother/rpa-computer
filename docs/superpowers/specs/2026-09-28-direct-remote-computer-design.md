# Computer Client → Host 直接联网

用户已确认目标并要求实现：Mac上的Claude只连接本地MCP Client，Client直接连Windows Host；SSH仅部署/启动/停止/取日志，不做端口转发。此次不修模型服务视觉链路，不新增Agent/LLM循环，不更改8个computer工具。

## 选择
- 仅放开现有明文TCP：拒绝，屏幕/键鼠/Token会明文传输。
- 新建HTTP/WS网关：可行但此阶段多一层协议，无必要。
- **TLS保护现有换行JSON-RPC + Token，本机编译Client提供stdio MCP**：采用。原loopback测试模式保留，本次不是公网托管服务。

## 接口
- `computer-host --remote-listen <IP:PORT> --tls-cert <server.pem> --tls-key <server.key> --token-file <host.token> [--log-file <PATH>] [--mock-backend]`
- `computer-client --connect <HOST:PORT> --ca-cert <ca.pem> [--server-name <certificate-DNS-name>] --token-file <client.token>`
- connect可用IP（--server-name显式指定证书身份）或DNS。缺省server-name是connect主机。仅TLS，无--insecure，不接受非loopback明文。
- MCP配置只含client命令、地址、证书/Token**文件路径**；Token不进argv或日志。stdin/stdout仍只承载JSON-RPC，图片content不解析改写。

## 安全与生命周期
使用rustls0.23 ring/std/tls12及pki-types PEM解析（按本机已缓存API核验）；完整信任链+证书名校验，Token仅TLS握手成功后发送。握手及认证总截止时间5秒、输入长度上限4096、常量时间比较，失败不创建桌面worker。

远端模式是Host内的TLS接入/进程监督层：每个已认证连接启动**同一个computer-host可执行文件的现有stdio模式**，不是任务Runner/Agent。这样复用全部现有取消、观察代际、去重、watchdog和nativeworker逻辑，不重新实现MCP。Child继承Windows交互式桌面会话；Windows子进程CREATE_NO_WINDOW（仅本进程拥有的stdio子进程），stderr到host日志。不往子进程传Token/TLS私钥。

一个Host只允许一个控制连接：执行中接受到的多余连接立即关闭、不排队等待获得控制。单个TLS状态只能由一个I/O循环持有；不可一把Mutex锁住blockingread饿死write。非阻塞socket+rustls状态机，stdio输入输出专用线程、有界chunk队列，队列满则明确断连并清理，不静默丢数据；TLS缓冲有界。网络EOF、TLS错误、本机Client stdinEOF、背压超限必须结束本连接并关闭child stdin，让原生runtime先清理；给child有界退出时间，超时只kill/reap该owned子进程。禁止旧child未退出就启动新child。成功清理后listener保持可用，下次连接创建全新worker。此为连续使用的远端服务，不再每次连接手动重启Host。

Client不自动重连/重放动作（无法确认是否执行）；连接失败输出stderr并非零退出。退出不等待无法取消的stdin线程无限join，安全终止owned网络连接。TLS连接未正确close_notify的截断视为错误，不能伪报完整成功；正常stdinEOF要有界处理关闭并尽力排空已经收到的响应，不用裸TCP SHUT_WR截断TLS。

本版本单共享Token，部署在受控LAN；不承诺多租户、用户级权限或公网抗DoS。不自动修改防火墙。证书用独立私有CA+DNS SAN服务器证书，首次通过可信部署通道分发CA与Token，私钥留Host；不要去信任任意远端递来的证书。

## 文件边界
Rust新remote模块（TLS配置、TLS pump、host监督、client），新增computer-client二进制。computer-host仅新增mode分发，remote分支在初始化桌面worker前进入。保留原stdio及loopback分支与测试。
脚本后续增加证书/Token初始化、Windows交互式remote启动/停止、sourcefree远端client打包示例，文档给两台机器命令。脚本不得覆盖非owned路径、不得改全局Claude设置或自动改防火墙。

## 验收
1. TLS+Token下真实本机进程端到端initialize/tools/list/open/observe/close，PNG原样可解码；mock明确标记。
2. wrong Token/CA/server-name、缺TLS、明文LAN均拒绝；未认证不启动stdio child。
3. 重连新session、额外客户端拒绝、取消可越过进行中请求、断连清理、child崩溃/EOF、stdout背压、进程退出有界，token无日志。
4. 全量cargo test/fmt/clippy、Windows交叉构建、现有Python协议与传输回归。
5. acer-win动态探测LAN地址，部署新产物、Session1计划任务，**无ssh -L/-R/-D**直接连接，取得真实PNG及Host日志。若防火墙阻拦如实报告，不擅改系统策略。
6. 模型看懂图片/计算器任务门禁与本次直连功能分开报告，不将本次网络通过描述为全项目GUI验收完成。
