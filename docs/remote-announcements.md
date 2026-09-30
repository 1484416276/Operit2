# 远程公告

公告来源与旧 Kotlin 项目相同：`https://operit.app/announcements/latest.json`。指针中的 `latestFile` 由标准 URL 解析器解析，支持相对路径和完整 HTTP(S) 地址。

## 当前项目的行为

- 公告面向全部用户，不按应用版本、构建渠道、用户语言或发布时间窗口筛选；旧协议的 `target`、`schedule` 不参与展示判断。
- `enabled` 控制公告发布/撤下；已确认版本及更早版本不再弹出。
- 正文直接读取 `content.locales[content.defaultLocale]`，不根据用户语言选择，不替换缺失文案。发布者必须提供该内容。
- 进入引导完成后的主界面以及应用恢复前台时检查公告。同一界面期间只允许一个请求或弹窗。
- 弹窗正文可滚动；确认倒计时限制为 0–30 秒；点击遮罩或返回键不能关闭。
- 点击确认后先保存 `acknowledged_version`，成功后关闭；保存失败保留弹窗并显示错误。
- 拉取、解析和存储错误明确记录，不转换成“无公告”，不使用缓存公告替代。

## 架构

`RemoteAnnouncementService` 使用 Core `HttpHost` 拉取数据，通过 `PreferenceStorageManager` 保存确认状态。Flutter 使用生成的 Core proxy 展示公告。未引入平台判断、独立 native/web 实现或直接文件访问。

偏好文件：`remote_announcement_preferences`；键：`acknowledged_version`。

Proxy 由 `operit-proxy-local` 的标准生成流程发现和生成。生成产物属于仓库现有忽略目录，不手工维护。
