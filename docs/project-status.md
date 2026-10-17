# 当前开发状态与合并记录

更新日期：2026-10-18（香港时间）。

## 统一开发入口

正式实现 #3–#7 已依次合入 `main`，后续从主分支建立短期功能分支。
不再把这五个阶段当作尚未合并的并行方案。集成提交
`1f4e8fbccb9f58b6f9862e053d2abaef287aefa3` 的完整文件树与已验证的 #7
`2592899c6feabe2fbd54f208a8fc22ee79e5979e` 相同：
`845f6c366ce4180ab608d21aa9e68d66ea7c47e0`。

| 变更 | 正式内容 | 主分支合并提交 |
|---|---|---|
| [#3](https://github.com/lancerct/imgcull/pull/3) | XMP 安全、照片配对、受保护恢复 | `255a478bf84bc1179a3ad8e262f6b26568798889` |
| [#4](https://github.com/lancerct/imgcull/pull/4) | 统一原尺寸解码与审阅证据 | `4fd48480ce9234996812816e493fde987c7603bd` |
| [#5](https://github.com/lancerct/imgcull/pull/5) | 可续跑审阅记录、组内约束与判断门槛 | `81ff5e3e6ee3338487a2c15d34dbbcffde1a475f` |
| [#6](https://github.com/lancerct/imgcull/pull/6) | LR 当前效果试导出与材料核验 | `b4653a347dc7c7c9407118fb870baaf0c96b07b9` |
| [#7](https://github.com/lancerct/imgcull/pull/7) | 冗余片比较、场景化反馈、允许多解的验证 | `1f4e8fbccb9f58b6f9862e053d2abaef287aefa3` |

合并保留原始提交的依赖关系；没有将上游阶段压成新提交后重新引入后续阶段。
每次合并均核对文件树与对应已验证分支一致，没有通过人工改代码解决冲突。

## 旧方案归档

- [#1](https://github.com/lancerct/imgcull/pull/1) 是早期 Python 记录器与 skill 试验。
  正式架构使用一个 Rust 引擎和一个审阅进度源，不合入旧运行时，也不安装旧 skill。
  有用的选片偏好已在正式策略、场景案例和文档中承接；未来 skill 仅作薄协调层。
- [#2](https://github.com/lancerct/imgcull/pull/2) 的历史审计报告归档在
  [首次审查](audits/2026-10-16-culling-review.md) 与
  [调查测试记录](audits/2026-10-17-probe-results.md)。
  旧调查测试验证“旧缺陷存在”，不能作为修复后的生产回归测试直接合入。
  两份历史报告的基线、结论和当时未完成事项保持原样，不代表当前问题状态。

## 已验证的边界

#7 精确提交的 [检查记录](https://github.com/lancerct/imgcull/actions/runs/39048225031)
通过 Linux 172 / Windows 169 项 Rust 测试，原有 14 项 Lua 插件契约测试、
Lua 语法、严格格式、锁定依赖构建、静态检查和 API 文档检查。
主分支合并后的运行结果以 [main 检查](https://github.com/lancerct/imgcull/actions/workflows/rust.yml?query=branch%3Amain) 为准。
工程测试通过不等于模型已经通过真实照片准确率评测。

## 尚未打通的完整流程

个人选片命令仍保留 `can_apply=false`：
真实 LR 当前效果与任务绑定、经过验证的模型原尺寸观看路径、语义分组与实际审图、
独立实拍校准、最终元数据计划和写回、独立导入 JPEG 的同步核验仍需完成。
试导出插件可生成材料；它不是已经完成的自动选片插件。
旧 `score/describe` 只用于概览式流程，不能代替个人选片的原尺寸复核。

详见 [架构](culling-architecture.md)、[反馈与比较](calibration-and-comparison.md)、
[LR 试导出](lightroom-export.md)。
上游安装器下载上游发布版，不包含本分支新增的个人工作流；请明确区分来源。
本次合并没有发布安装包，也没有修改实际图库、XMP 或已安装插件。

## 后续迭代原则

从当前 `main` 开始，每轮保持单一目的；只在确有未合入依赖时临时叠放分支。
完成相应检查后及时合并并关闭被替代的试验，不把整个产品完工当作基础代码合并条件。
模型能力与实际摄影判断需要另外验证；数据安全和未完成能力的保护门槛继续保留。
