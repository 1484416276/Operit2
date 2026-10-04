/* METADATA
{
    "name": "operit_editor",
    "display_name": {
        "zh": "Operit 平台编辑器",
        "en": "Operit Platform Editor"
    },
    "description": {
        "zh": "Operit2 平台编辑与排查手册。使用当前版本 core command 管理包与配置，并核验真实状态。",
        "en": "Operit2 platform editing and troubleshooting guide for the current core command surface."
    },
    "enabledByDefault": false,
    "category": "System",
    "tools": [
        {
            "name": "operit_editor",
            "description": {
                "zh": "读取 Operit2 平台编辑手册。真正的配置、包、Skill、MCP、模型、聊天、工作区操作请直接调用系统工具 execute_cli_command。",
                "en": "Read the Operit2 platform editing guide. Use the system execute_cli_command tool for package, skill, MCP, model, chat, and workspace operations."
            },
            "parameters": [
                {
                    "name": "query",
                    "description": {
                        "zh": "可选，说明本次要编辑或排查的目标。",
                        "en": "Optional editing or troubleshooting target."
                    },
                    "type": "string",
                    "required": false
                }
            ]
        }
    ]
}*/

type OperitEditorParams = {
    query?: string;
};

const OPERIT_EDITOR_GUIDE = `
# Operit2 平台编辑器

这个包只提供当前 Operit2 平台编辑手册。执行动作使用系统工具 execute_cli_command，参数是 CLI 字符串数组，不含 operit2 可执行文件名。脚本内对应 Tools.SoftwareSettings.exec(args)。Core command 不等于终端 shell 命令，本包不提供脚本片段执行工具。

ToolPkg API 版本与跨平台要求：

- Operit2 新建 ToolPkg 显式声明 api_version 为 2.0.0；其对 1.0.0 和 1.0.1 的加载支持并不完整。Operit1 完整支持 1.0.0 和 1.0.1，这两个版本主要面向 Android，是旧版 API 形式。
- 2.0.0 作者须注重跨平台兼容性。基本所有公共接口都兼容多平台，常规功能使用统一接口即可；遇到平台特异接口时，明确适用范围，并考虑、验证其他平台的安装、界面和核心功能。
- schema_version 是清单格式版本，api_version 是 API 契约版本，version 是插件自身发布版本。旧包迁移必须检查实际接口、路径与平台行为，不能仅修改版本号或凭导入成功宣称完整兼容。

常用入口：

- 查看帮助：空数组查看总入口；["package", "help"] 查看包命令；["skill"]、["tool"]、["workspace"] 查看对应命令用法。
- 包管理：["package", "dir"]、["package", "import", "<artifact-host-path>"]、["package", "delete", "<name>"]、["package", "list"]、["package", "more"]、["package", "load", "<name>"]、["package", "show", "<name>"]、["package", "enable", "<name>"]、["package", "disable", "<name>"]、["package", "use", "<name>"]、["package", "exec", "<package:tool>", "<params-json>"]。
- Skill：["skill", "dir"]、["skill", "list"]、["skill", "show", "<name>"]、["skill", "visible", "<name>", "true"]、["skill", "visible", "<name>", "false"]、["skill", "errors"]。
- MCP：["mcp", "dir"]、["mcp", "list"]、["mcp", "show", "<name>"]、["mcp", "enable", "<name>"]、["mcp", "disable", "<name>"]、["mcp", "start", "<name>"]、["mcp", "tools", "<name>"]。
- 模型：["model", "list"]、["model", "show", "<id>"]、["model", "function-list"]、["model", "function-show", "<type>"]、["model", "function-set", "<type>", "<provider-id>", "<model-id>"]。
- 偏好设置：["prefs", "show"]、["prefs", "thinking", "on"]、["prefs", "stream", "on"]、["prefs", "media-history", "<image-user-turns>", "<media-user-turns>"]、["prefs", "mcp-timeout", "<seconds>"]。
- 日志：["log", "show"]、["log", "package"]、["log", "path"]、["log", "clear"]。
- 工具：["tool", "list", "public"]、["tool", "show", "<name>"]、["tool", "exec", "<name>", "<params-json>"]。
- 工作区：["workspace", "list"]、["workspace", "commands", "<chat-id>"]、["workspace", "run", "<chat-id>", "<command-id>"]、["workspace", "bind-default", "<chat-id>"]。

插件创作约定：

- 使用 PackageBuilder skill 中随包携带的当前版本类型定义。
- 先阅读 PackageBuilder/references/PLUGIN_CREATION_WORKFLOW.md，用 ["skill", "show", "PackageBuilder"] 确认真实 Skill 目录，读取终端 host 信息并核验实际可写开发目录。不要写死平台路径。
- 区分 Tools.Files 的 VFS 路径、终端路径与 package import 的 FileSystemHost 路径，核对它们指向的真实文件。package dir 是安装存储目录，不是源码目录。
- 已安装的 PackageBuilder 不会自动更新附件。更新资料前备份用户修改并取得确认，再用 skill delete/load/visible/show 明确重新安装与核验。
- 包 id 在首次确定后保持不变。
- 使用终端完成源码开发与构建；使用当前 runtime 的 core command 完成安装、配置核验与工具测试。
- 首次安装走 package import，随后启用并重新读取 package list 核验 enabled 状态；通过 package show 获取实际工具名，再用 package exec 测试。UI、hook 和 provider 在对应应用场景验证，日志用 log package 查看。
- package import 拒绝重复包名。同 ID 更新需先完成构建，记录启用配置，并取得用户对删除、重新导入和恢复配置的确认；任一步失败即停止。
- 小片段调试写成有 METADATA 和导出函数的测试包，通过上述安装与执行流程验证。

包系统说明：

- 内置包来自应用内置资源。
- 准内置包来自应用内资源中的 external 候选，查看用 ["package", "more"]，加入加载列表用 ["package", "load", "<name>"]。
- 当前会话调用某个包前，用 ["package", "use", "<name>"] 让 runtime 激活它。
- ToolPkg 子包由包系统解析和展示，不手写另一套识别逻辑。

执行原则：

- 先用对应 core command 查看真实状态，再执行修改命令；不能把调用完成或命令回显当作实际操作成功。
- 需要变更用户配置、启停包、启停 MCP、删除资源时，先向用户确认。
- 不从云端拉取 PackageBuilder 类型；使用当前软件随包携带的类型。
`.trim();

/** Returns the current platform-editing guide without executing mutations. */
async function operit_editor(params: OperitEditorParams = {}) {
    const query = params.query?.trim();
    if (!query) {
        return OPERIT_EDITOR_GUIDE;
    }
    return `目标：${query}\n\n${OPERIT_EDITOR_GUIDE}`;
}

exports.operit_editor = operit_editor;
exports.main = operit_editor;
