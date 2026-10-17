> 历史归档：本文描述旧基线及当时的审查状态，非当前版本的缺陷清单。当前集成状态见[项目状态](../project-status.md)。源记录来自 [PR #2](https://github.com/lancerct/imgcull/pull/2)，正文保持原样。

# 审计调查测试执行记录

验证日期：2026-10-17（香港时间）。

- 上游基线：022c4161302aa29a5a230c0469814507e97e385b。
- 审计提交：18f85a242123e3c32f2466dfa50eee05fa588961。
- [GitHub Rust 构建和测试](https://github.com/lancerct/imgcull/actions/runs/38930353886)：成功。
- Ubuntu 运行项目真实 Rust 接口；原有测试及新增 7 项调查测试全部通过。
- 调查测试通过表示其所断言的**缺陷已被重现**，不表示缺陷已修复，也不表示图库可安全使用。
- 此次验证没有调用模型、没有使用或修改实际照片。生产源代码与 main 相同。
- 仍未完成：真实 Lightroom/JPEG 兼容矩阵、EXIF 方向实图验证、依赖漏洞数据库扫描。

日志节选（时间为 UTC）：

```text
2026-10-16T16:16:22.5770298Z test probe_non_score_json_can_become_persistent_one_star_score ... ok
2026-10-16T16:16:22.5771033Z test probe_model_identifier_is_not_xml_escaped ... ok
2026-10-16T16:16:22.7049207Z test probe_escaped_caption_is_double_escaped_on_unrelated_update ... ok
2026-10-16T16:16:22.7049826Z test probe_single_quoted_rating_is_unread_and_duplicated ... ok
2026-10-16T16:16:22.7050201Z test probe_self_closing_description_places_attributes_after_slash ... ok
2026-10-16T16:16:22.7127790Z test probe_second_backup_replaces_first_backup ... ok
2026-10-16T16:16:22.7137443Z test probe_stale_snapshot_overwrites_intervening_develop_edit ... ok
2026-10-16T16:16:22.7138100Z test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
```

主报告中关于调查测试“执行前”的文字是提交当时的状态，本记录补充最终结果。
