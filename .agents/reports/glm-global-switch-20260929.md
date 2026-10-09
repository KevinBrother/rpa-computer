# Claude 全局模型迁移 — 2026-09-29

用户明确要求将原 Kimi 任务切到 GLM，并纠正“仅本次指定、不改全局”的理解。本次已修改两台机器的用户级 Claude 配置，不是仅修改项目调用参数。

| 主机 | 配置 | 默认 model | fable 原 Kimi 路由改为 |
|---|---|---|---|
| 本机 macOS | `/Users/caojunjie/.claude/settings.json` | sonnet | `claude-sonnet-5` / glm-5.3-flash |
| acer-win | `C:\Users\Administrator\.claude\settings.json` | sonnet | `claude-sonnet-4-6` / glm-5.3-flash |

沿用各自主机已经验证的 sonnet 路由，不能混用底层别名。保留 sonnet、haiku、opus 映射及认证/网关等其他设置；不输出凭据。旧 fable 调用现在也指向 GLM。

备份：
- 本机 `settings.json.before-glm-20260929-100529.bak`（同目录，权限 0600）。
- 远端 `settings.json.before-glm-20260929-100533.bak`（同目录，沿用配置 ACL）。

实际验证：两端均启动**不带 --model**、无工具、无 MCP 的新 Claude CLI 会话；响应 message.model 均为 `glm-5.3-flash`，exit=0，回答 OK。证据 `.agents/runs/glm-global-local-probe-20260929.jsonl` 与 `.agents/runs/glm-global-remote-probe-20260929.jsonl`。模型身份是服务响应标签，不是供应商证明；既有已运行会话可能仍保留启动时设置，新会话已验证生效。

本项目同时将黑盒 runner 显式 pin sonnet；远端新 helper 默认且只接受 sonnet；原历史 helper/历史 Kimi 转录保留，不改写旧证据。实现 worker 本身响应模型也是 GLM。未 commit/push。
