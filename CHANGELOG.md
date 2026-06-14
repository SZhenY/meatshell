# Changelog / 更新日志

All notable changes are documented here. 本文件记录所有重要变更。
中英对照（English first, 中文在后）.

## [Unreleased]

## [0.4.3] - 2026-06-14

### Fixed / 修复

- **Wide CJK glyphs are grid-aligned in the terminal.** With a Chinese path, the
  trailing `/` after `ll`, the cursor after `cd`, and the prompt `$` no longer
  overlap or drift away from the last CJK character — each wide character now
  occupies exactly its two terminal cells.
  **终端里的中文(宽字符)对齐到网格。** 中文路径下,`ll` 后目录名的 `/`、`cd`
  之后的光标、提示符 `$` 不再与中文末字重叠或拉开很远——每个宽字符现在正好
  占它的两个终端格。

## [0.4.1] - 2026-06-14

### Added / 新增

- **Run / copy / delete actions on command history (#96).** Each entry in the
  command-history dropdown now has run (▶), copy (⧉) and delete (🗑) buttons —
  run executes it immediately, copy puts it on the clipboard, delete removes it.
  **命令历史的运行 / 复制 / 删除 (#96)。** 历史下拉里每条记录新增 ▶ 运行、
  ⧉ 复制、🗑 删除按钮——运行即时执行,复制到剪贴板,删除移除该条。

- **Default-collapse settings for the sidebars (#78).** Settings → Interface →
  Sidebars adds two checkboxes to collapse the left resource panel and the bottom
  SFTP panel on startup — handy on low-spec jump hosts.
  **侧栏默认收起设置 (#78)。** 设置 → 界面 → 侧栏 新增两个复选框,可在启动时
  收起左侧资源面板和底部 SFTP 面板——适合低配跳板机。

## [0.4.0] - 2026-06-14

### Added / 新增

- **SSH port forwarding / tunnels (#56).** Per-session tunnels configured in the
  session dialog's Advanced section: local (-L), remote (-R) and dynamic
  (-D / SOCKS5). They auto-establish on connect and tear down on disconnect.
  **SSH 端口转发 / 隧道 (#56)。** 在会话对话框「高级」里按会话配置:本地 -L、
  远程 -R、动态 -D（SOCKS5）。连接时自动建立,断开时拆除。

- **Quick commands, command box & history (#55).** A command bar below the
  terminal: save named commands and click to run them, type into a command box
  (with an "all sessions" broadcast toggle), and recall history with ↑/↓.
  **快捷命令、命令输入框与历史 (#55)。** 终端下方的命令栏:保存命名命令点击即发、
  命令输入框（含「所有会话」群发开关）、↑/↓ 回溯历史。

- **Remote process monitor (#23).** The server-resource panel gains a "Processes"
  button that opens a read-only table (PID / user / CPU% / MEM% / command), sorted
  by CPU and refreshed live.
  **远端进程监控 (#23)。** 服务器资源面板新增「进程」按钮,打开只读进程表
  （PID / 用户 / CPU% / 内存% / 命令）,按 CPU 排序、实时刷新。

- **Encrypted private keys (#90).** Key auth now accepts a passphrase for
  encrypted private keys.
  **加密私钥 (#90)。** 私钥认证支持为加密私钥输入密码短语。

### Fixed / 修复

- **CJK rendering (#54).** Chinese (especially isolated punctuation like 、：（）)
  no longer renders as tofu in editable inputs or the terminal — editable fields
  and the window now use a CJK-capable font, and CJK terminal spans fall back
  correctly.
  **中文渲染 (#54)。** 输入框和终端里的中文（尤其「、：（）」这类孤立标点）不再
  显示为方块——可编辑控件与窗口改用支持 CJK 的字体,终端的 CJK 片段也正确回退。

- **SFTP cd-follow under zsh (#91).** The SFTP panel now follows `cd` under zsh
  (and other shells), not just bash — the cwd notification is registered via the
  shell's proper hook instead of bash's `PROMPT_COMMAND` only.
  **zsh 下 SFTP 跟随 cd (#91)。** SFTP 面板现在在 zsh（及其它 shell）下也能跟随
  `cd`,不再只支持 bash——按各 shell 正确的钩子注册 cwd 通知。

- **Compact, scrollable session dialog.** Proxy and port-forwarding settings are
  collapsed under an "Advanced" toggle, and the dialog scrolls instead of being
  clipped when it would exceed the window.
  **会话对话框更紧凑、可滚动。** 代理与端口转发收进「高级」折叠;对话框超出窗口
  时内部滚动而非被截断。

## [0.3.8] - 2026-06-12

### Added / 新增

- **Confirm before closing when there are active sessions (#88).** Double-clicking
  the title-bar icon (or X / Alt+F4) no longer silently drops live sessions — a
  confirm dialog appears; with no sessions the window closes as before.
  **有活动会话时关闭前先确认 (#88)。** 双击标题栏图标(或点 X / Alt+F4)不再静默
  断开正在进行的会话——会弹出确认框;没有会话时则照旧直接关闭。

- **"Always ask where to save" download option (#87).** Settings → Interface →
  Download adds a checkbox (default off); when on, every download prompts for the
  folder instead of using the preset.
  **「总是询问保存去何处」下载选项 (#87)。** 设置 → 界面 → 下载 新增复选框
  (默认关闭);勾选后每次下载都询问保存位置,而非直接用预设目录。

- **The transfers popup opens automatically when a download starts**, so progress
  is visible without opening it by hand.
  **下载开始时自动弹出传输面板**,无需手动打开即可看到进度。

- **Capped diagnostic log file (groundwork for #86).** Writes to
  `<config_dir>/error.log` at WARN and above — a single file capped at 5 MiB that
  auto-overwrites when full — so users can share what went wrong (e.g. a bastion
  disconnect reason) without setting RUST_LOG.
  **容量受限的诊断日志文件(为 #86 铺路)。** 写入 `<配置目录>/error.log`
  (WARN 及以上)——单文件、上限 5 MiB、满了自动覆盖——用户无需设置 RUST_LOG 即可
  把出错信息(如堡垒机断开原因)发来。

### Fixed / 修复

- **Settings checkboxes now persist visually after reopening the dialog (#87).**
  "Always ask where to save" and "SFTP follows cd" used a one-way binding, so
  reopening the settings dialog (without restarting) showed the stale state even
  though the value was saved; switched to a two-way binding.
  **设置里的复选框重新打开对话框后状态保持 (#87)。** 「总是询问保存去何处」和
  「SFTP 跟随 cd」原用单向绑定,不重启 app 时重开设置对话框会显示旧状态(尽管值
  已保存);改为双向绑定。

## [0.3.7] - 2026-06-12

### Added / 新增

- **Right-click anywhere in the SFTP file list (#84).** The list fills the panel
  height, so right-clicking the whitespace below the items works too; item
  actions grey out when nothing was hit, leaving new folder / new file / refresh.
  **SFTP 文件列表任意处可右键 (#84)。** 列表填满面板高度,条目下方空白也能右键;
  未点中条目时,条目相关操作置灰,仅保留 新建文件夹 / 新建文件 / 刷新。

- **Visual permissions dialog (#84).** Permissions opens a checkbox matrix
  (Owner/Group/Other × Read/Write/Execute) prefilled from the file's current
  mode, instead of typing an octal string.
  **可视化权限对话框 (#84)。** 「权限」打开勾选矩阵(所有者/组/其他 × 读取/写入/
  执行),按文件当前权限预填,不再手输八进制。

- **Open / Edit externally (#81).** New SFTP context-menu items hand the file to
  the OS default app (e.g. VS Code) for syntax highlighting / large files; edit
  mode watches the temp copy and re-uploads on change.
  **外部程序查看 / 编辑 (#81)。** SFTP 右键新增菜单项,把文件交给系统默认程序
  (如 VS Code)打开以获得语法高亮 / 处理大文件;编辑模式监听临时副本并自动重传。

- **Line numbers in the built-in editor (#81).**
  **内置编辑器加行号 (#81)。**

- **Upload a whole folder from the Upload button (#85).** The button now offers
  "Upload file" (multi-select) / "Upload folder".
  **上传按钮支持整个文件夹 (#85)。** 现在提供「上传文件」(可多选) /「上传文件夹」。

- **Linux ARM64 release build (#82)** and **AUR packaging scaffolding (#61).**
  **Linux ARM64 发布构建 (#82)** 与 **AUR 打包脚手架 (#61)。**

### Changed / 变更

- **Downloads default to the user's Downloads folder (#85)** instead of prompting
  every time; the settings button reads "Choose save path".
  **下载目录默认设为用户的「下载」文件夹 (#85)**,不再每次询问;设置按钮文案改为
  「选择保存路径」。

### Fixed / 修复

- **No more error under fish (#71).** The OSC 7 prompt injection is guarded with
  `test -z "$FISH_VERSION"`, so it's a no-op under fish (which emits OSC 7 on its
  own, keeping cd-follow working) and unchanged under bash/zsh/sh.
  **fish 下不再报错 (#71)。** OSC 7 提示符注入加了 `test -z "$FISH_VERSION"` 守卫,
  在 fish 下为空操作(fish 自带 OSC 7,cd 跟随照常),bash/zsh/sh 行为不变。

## [0.3.5] - 2026-06-12
- 我和claude都快冒烟了，不想写，让我摆烂一会吧。Claude and I are both about to explode with frustration. We don't want to write anymore. Let me just laze around for a while.

## [0.3.3] - 2026-06-11

### Added / 新增

- **In-app new-version notification (#48).** On startup a background thread
  checks the GitHub releases API; if a newer version exists, a dismissible
  top-centre banner offers a Download button (opens the release page). Purely
  informational — the app keeps working on the current version, never forced.
  **应用内新版本通知 (#48)。** 启动时后台线程查询 GitHub releases API,若有更
  新版本则在顶部居中显示一个可关闭的横幅,带"下载"按钮(打开 release 页)。纯
  提示性质——应用照常在当前版本运行,绝不强制更新。

- **Editable SFTP path bar with copy & paste-to-jump (#54).** The path bar is
  now an input: type or paste a path and press Enter to jump there, plus a copy
  button and a paste-and-jump button.
  **可编辑的 SFTP 路径栏,支持复制和粘贴跳转 (#54)。** 路径栏现在是输入框:输入
  或粘贴路径后回车即跳转,另有复制按钮和粘贴跳转按钮。

### Fixed / 修复

- **SFTP panel no longer gets stuck "loading" after manual navigation (#59).**
  The panel only follows a real cd now (cwd actually changed), not the OSC 7 that
  every prompt re-emits — so manual browsing isn't overridden and a later cd
  reloads correctly instead of hanging.
  **SFTP 面板手动导航后不再卡"加载中" (#59)。** 面板现在只跟随真正的 cd(cwd 确
  实变化),而非每个命令提示符都重发的 OSC 7——手动浏览不被覆盖,之后的 cd 也能
  正确重新加载而非卡住。

- **SFTP path bar renders Chinese instead of tofu (#54).** The embedded Cascadia
  Mono has no CJK glyphs and native TextInput doesn't glyph-fallback; editable
  inputs now use a CJK-capable system font.
  **SFTP 路径栏正常显示中文,不再是豆腐块 (#54)。** 嵌入的 Cascadia Mono 没有
  CJK 字形,而原生 TextInput 不做字形回退;可编辑输入现改用含 CJK 的系统字体。

## [0.3.2] - 2026-06-11

### Added / 新增

- **MSI installer for Windows (best-effort).** Built by cargo-wix with a
  WixUI_InstallDir wizard, so you can change the install location during setup.
  **Windows MSI 安装包(尽力而为)。** 由 cargo-wix 构建,带 WixUI_InstallDir
  向导,安装时可更改安装位置。

- **Explicit session folders (#41).** Groups are now first-class: create / rename
  / delete them, keep empty folders, and right-click a group header to manage it.
  Right-click a session to "move to" any group (incl. empty ones).
  **显式会话文件夹 (#41)。** 分组成为一等公民:可新建 / 重命名 / 删除,可保留空
  文件夹,右键分组标题进行管理;右键会话可"移动到"任意分组(含空文件夹)。

### Changed / 变更

- **Interface settings dialog** is now draggable (by its title bar), truly modal
  (background click no longer closes it), and its font controls no longer span
  the full pane.
  **「界面」设置对话框**现在可拖动(拖标题栏)、真模态(点背景不再关闭),字体控件
  也不再占满整个面板。

- **Session right-click menu reordered:** Edit / Duplicate / Delete above the
  divider, the "move to group" list below it with a header hint.
  **会话右键菜单重排:** 编辑 / 复制副本 / 删除在分割线上方,"移动到分组"列表带
  提示在下方。

### Fixed / 修复

- **Wide (CJK) characters no longer misalign the cursor (#60).** A wide glyph's
  blank continuation cell was being filled with a space, pushing the line and
  cursor one cell right per character; it now emits nothing.
  **宽(CJK)字符不再导致光标错位 (#60)。** 宽字形的空白延续格此前被补了空格,
  每个字符把行和光标右推一格;现在延续格不输出任何内容。

- **Download & settings popups close on click-outside.** A full-window backdrop
  under each popup closes it when you click outside.
  **下载/设置弹窗点击外部即关闭。** 每个弹窗下方铺一层全窗背景,点击外部即关闭。

- **Disk tooltip clears when the pointer leaves the panel**, and the OSC 7
  shell-integration command no longer leaves an extra blank prompt on connect.
  **磁盘 tooltip 在指针离开面板时消失**,OSC 7 shell 集成命令也不再在连接时留下
  多余的空提示符。

## [0.3.1] - 2026-06-10

### Security / 安全

- **Restrict sessions.json to owner-only on Unix (#34).** The config file holds
  (encrypted) credentials, so it is now written with mode 0600 — like
  secret.key — and other local accounts can't read it. Windows %APPDATA% is
  already owner-restricted by default ACLs.
  **将 sessions.json 限制为仅属主可读(Unix)(#34)。** 配置文件含(加密的)凭据,
  现在以 0600 权限写入(与 secret.key 一致),其它本地账户无法读取。Windows 的
  %APPDATA% 默认 ACL 已限制为属主。

### Build / 构建

- **Build macos-x86_64 by cross-compiling on Apple Silicon runners.** The
  dedicated Intel (macos-13) runners queue for ages and often time out, so the
  x86_64 Mac binary is now cross-compiled on a plentiful macos-14 runner.
  **在 Apple Silicon runner 上交叉编译 macos-x86_64。** 专用 Intel(macos-13)
  runner 排队极久且常超时,x86_64 Mac 二进制改为在充足的 macos-14 runner 上
  交叉编译。

## [0.3.0] - 2026-06-10

### Added / 新增

- **Interface settings — terminal font & size.** The gear menu's new "Interface"
  item opens a modal dialog (27% nav / 73% content) whose Font page lets you pick
  from the system's installed monospace fonts and set the size (8–32 px) with a
  live preview. Both apply immediately (cell size / cols / rows re-derive) and
  persist.
  **「界面」设置 —— 终端字体与字号。** 齿轮菜单新增「界面」项,打开模态对话框
  (27% 导航 / 73% 内容),「字体」页可从系统已安装的等宽字体中选择并设置字号
  (8–32 px),带实时预览。两者即时生效(cell 尺寸 / 列 / 行重新推导)并持久化。

- **Session folders / groups (#41).** Each session can belong to an optional
  group; Quick Connect shows folder headings (sorted; ungrouped under "default")
  that collapse when clicked. Right-click a session to move it to another group
  or duplicate it; right-click a group header to create a new group.
  **会话文件夹 / 分组 (#41)。** 每个会话可归入可选分组;「快速连接」按分组显示
  文件夹标题(排序;未分组归入「default」),点击标题可折叠。右键会话可移动到其它
  分组或复制副本;右键分组标题可新建分组。

- **Recursive SFTP folder transfer (#50).** Upload, download and delete whole
  directory trees: drag a folder onto the panel to upload, right-click a folder
  to download, and delete now removes non-empty directories too.
  **SFTP 文件夹递归传输 (#50)。** 可上传、下载、删除整个目录树:把文件夹拖到面板
  上传,右键文件夹下载,删除现在也能删非空目录。

- **Collapsible panels (#41).** Both the left sidebar and the SFTP panel can be
  minimized to reclaim screen space.
  **可折叠面板 (#41)。** 左侧栏与 SFTP 面板都可最小化以腾出屏幕空间。

- **Export / import connections (#46).** New "Export connections" / "Import
  connections" in the settings menu to migrate sessions between machines. The
  exported JSON keeps host/user/port in plaintext and obfuscates only the
  password with a built-in key, so it opens on any machine; key-auth sessions
  export the key path. Imports skip duplicates (host+user+port+kind).
  **导出 / 导入连接 (#46)。** 设置菜单新增「导出连接」「导入连接」,用于在多台
  机器间迁移会话。导出的 JSON 中 host/user/port 为明文,仅密码用内置 key 混淆,
  因此在任意机器都能打开;密钥认证的会话导出私钥路径。导入会跳过重复
  (host+user+port+kind)。

- **Pick SOCKS5 or HTTP proxy type in the session dialog (#46).** The proxy
  field is now a None / SOCKS5 / HTTP selector plus a `host:port` input. HTTP
  CONNECT was already supported by the backend but wasn't selectable in the UI.
  The stored proxy URL format is unchanged.
  **会话对话框选择 SOCKS5 或 HTTP 代理类型 (#46)。** 代理项改为 不使用 / SOCKS5 /
  HTTP 选择器加 `host:port` 输入框。HTTP CONNECT 后端早已支持,只是 UI 无法选择。
  存储的代理 URL 格式不变。

- **Confirmation prompt before deleting a remote file (#28).** SFTP delete is
  irreversible (there is no trash), so the context-menu *Delete* now asks for
  confirmation — showing the full path — before removing anything; a misclick
  no longer silently destroys a file.
  **删除远程文件前先确认 (#28)。** SFTP 删除不可撤销(没有回收站),右键菜单的
  「删除」现在会先弹出确认框(显示完整路径)再执行,误点不会再悄悄删掉文件。

- **Serial port sessions (#14, #17).** New session type for connecting to
  switches, routers and embedded devices over a serial console. Pick
  **Serial** in the session dialog and set the port (`COM3`, `/dev/ttyUSB0`),
  baud rate, data/stop bits, parity and flow control. The serial line reuses
  the full terminal pipeline (output, input, scrollback, copy/paste); SFTP and
  the resource monitor are not applicable and are hidden.
  **串口会话 (#14, #17)。** 新增串口会话类型,用于通过串口控制台连接交换机、
  路由器和嵌入式设备。在会话对话框选择 **串口**,填写串口号(`COM3`、
  `/dev/ttyUSB0`)、波特率、数据/停止位、校验位和流控。串口复用完整的终端管线
  (输出、输入、回滚、复制粘贴);SFTP 和资源监控不适用,已隐藏。

- **Telnet sessions (#17).** New session type for legacy gear that only speaks
  Telnet. Handles RFC 854 option negotiation (suppress-go-ahead / echo /
  window-size), strips IAC sequences from the stream, and tunnels through the
  same SOCKS5 / HTTP proxy as SSH when configured.
  **Telnet 会话 (#17)。** 新增 Telnet 会话类型,用于只支持 Telnet 的老旧设备。
  处理 RFC 854 选项协商(抑制 Go-Ahead / 回显 / 窗口大小),从数据流中剥离 IAC
  序列,并可经与 SSH 相同的 SOCKS5 / HTTP 代理隧道连接。

### Performance / 性能

- **Pipelined SFTP upload (#16).** Uploads now keep ~32 WRITE requests in flight
  on a dedicated SFTP channel instead of writing one chunk and waiting for each
  ack, hiding the round-trip latency that made transfers ~15x slower than `scp`.
  Out-of-order completion is safe (every chunk carries its absolute offset).
  **SFTP 上传流水线化 (#16)。** 上传改为在专用 SFTP 通道上保持约 32 个 WRITE 请求
  并发在途,而不是写一块等一块的 ack,消除了让传输比 `scp` 慢约 15 倍的往返延迟。
  乱序完成也安全(每块都带绝对偏移)。

### Fixed / 修复

- **Drag-select no longer auto-scrolls within the visible area (#41).** Selecting
  text now only scrolls once the drag leaves the viewport edge, so the view no
  longer jumps and the selection no longer snaps to an edge row.
  **拖动选择在可见区内不再自动滚动 (#41)。** 现在仅当拖动离开视口边缘才滚动,
  视图不再乱跳、选区也不再吸附到边缘行。

- **Alt no longer clears the typed command (#43).** Slint encodes a lone
  modifier key as a C0 code point (Alt=0x12); pressing Alt (e.g. to Alt+Tab
  away) sent ESC+0x12 to the PTY, which bash/readline treated as Meta and
  discarded the input line. Bare modifier codes are now dropped, with a guard
  that preserves a real Ctrl+P..Ctrl+X.
  **按 Alt 不再清空已输入命令 (#43)。** Slint 把单独的修饰键编码成 C0 码位
  (Alt=0x12);按 Alt(如 Alt+Tab 切换)会向 PTY 发送 ESC+0x12,被 bash/readline
  当作 Meta 而丢弃输入行。现在丢弃单独的修饰键码位,并保留真实的 Ctrl+P..Ctrl+X。

- **Multi-line / backslash-continued commands now paste intact.** Pasted text
  kept its CRLF/LF line breaks, but the terminal expects CR for Enter, so CRLF
  made the shell see two breaks per line and end a `\`-continued command early.
  Pasted line endings are normalised to a single CR.
  **多行 / 反斜杠续行命令现在能完整粘贴。** 粘贴文本保留了 CRLF/LF 换行,而终端
  回车应为 CR,CRLF 会让 shell 每行看到两个换行、提前结束 `\` 续行命令。现在把
  粘贴的换行统一规范为单个 CR。

- **Session dialog no longer mis-lays-out when switching connection type.** The
  card had a fixed height, so Telnet/Serial (with fewer fields) left slack space
  that stretched inputs apart. The card height now follows its content.
  **切换连接类型时会话对话框不再排版错乱。** 卡片此前固定高度,Telnet/串口
  (字段更少)会留出空白把输入框撑开。现在卡片高度跟随内容。

- **Copy/paste works on Wayland sessions (#47).** arboard's default Linux
  backend is X11, which fails on Wayland (Debian sid / KDE) without XWayland.
  Enabled arboard's native `wayland-data-control` backend, and copy now uses
  set().wait() so the selection survives after the clipboard handle is dropped.
  **Wayland 会话下复制粘贴恢复可用 (#47)。** arboard 默认 Linux 后端是 X11,在
  无 XWayland 的 Wayland(Debian sid / KDE)下失效。启用 arboard 原生
  `wayland-data-control` 后端,复制改用 set().wait() 使选区在剪贴板句柄 drop 后
  仍然有效。

- **Hide the shell-integration command from the terminal.** meatshell injects a
  one-line `PROMPT_COMMAND` (OSC 7) on connect so the SFTP panel can follow the
  terminal's working directory. Its echo used to show up on every connect (and
  pollute shell history); the line now carries a leading space (kept out of
  history) and its echo is stripped from the output before display.
  **隐藏 shell 集成注入命令。** meatshell 连接时会注入一行 `PROMPT_COMMAND`
  (OSC 7),让 SFTP 面板跟随终端当前目录。此前它的回显每次连接都显示在终端
  (并污染命令历史);现在该行带前导空格(不进历史),回显也会在显示前被剥离。

- **Dragging the SFTP panel up no longer clears terminal output (#18).** vt100's
  shrink truncated the grid from the bottom, dropping the most recent output;
  before shrinking we now save the top rows to scrollback and scroll so the
  bottom (recent) rows stay visible. Two follow-ups: (1) the shrink now only
  scrolls off as many rows as needed to keep the cursor visible, so rapid
  up/down dragging on a not-yet-full screen no longer pushes the prompt into
  scrollback and strands the cursor at the top (also reported as #24); (2) drag-selection is now stored
  in absolute scrollback coordinates, so selecting from the top of the history
  down through several screens copies every line instead of losing everything
  above the final window when the view auto-scrolls.
  **上拉 SFTP 面板不再清空终端输出 (#18)。** vt100 缩小时从底部截断,丢掉最近输出;
  现在缩小前把顶部行存入回滚区并滚动,使底部(最近)行保持可见。两处后续修复:
  (1) 缩小时只滚走"保持光标可见所需"的行数,疯狂上下拖动未填满的屏幕时不再把
  提示符推进回滚区、光标卡在顶部;(2) 拖选改用绝对回滚坐标存储,从历史顶部往下
  跨多屏选择时能复制到每一行,而不是在视图自动滚动后丢掉最后一屏以上的内容。

### Security / 安全

- **Redact proxy credentials and zero them in memory (#32).** The HTTP/SOCKS
  proxy password is now wrapped in `Secret` (zeroed on drop) and ProxyConfig has
  a manual Debug that redacts auth, so credentials can't leak via {:?}/tracing
  or linger in core dumps.
  **代理凭据脱敏并在内存清零 (#32)。** HTTP/SOCKS 代理密码改用 `Secret` 包装
  (drop 时清零),ProxyConfig 手写 Debug 对凭据脱敏,使其无法经 {:?}/tracing
  泄露或残留于 core dump。

- **Validate HostName when importing ~/.ssh/config (#33).** Imported HostName
  values are now checked — IP literals and DNS hostnames accepted; shell
  metacharacters, whitespace and scheme prefixes rejected — and invalid entries
  are skipped with a warning.
  **导入 ~/.ssh/config 时校验 HostName (#33)。** 现在校验导入的 HostName(接受
  IP 字面量与 DNS 域名,拒绝 shell 元字符、空白与协议前缀),非法条目跳过并告警。

- **Harden the remote resource monitor against a hostile server (#27).** The
  monitor runs a small loop over an SSH exec channel. It now (1) resets `PATH`
  to the standard system dirs so a server with a hijacked `PATH`/`BASH_ENV`
  can't shadow `awk`/`cat`/`df`/`sleep`; (2) caps the reassembly buffer at 1 MiB
  so a server that streams data without the sync marker can't exhaust memory;
  and (3) parses `/proc` and `df` output with saturating arithmetic and a
  64-row cap per sample, so crafted huge values or a flood of fake interfaces
  can't overflow-panic or swamp the sidebar.
  **加固远程资源监控以防恶意服务器 (#27)。** 监控通过 SSH exec 通道跑一个小循环。
  现在:(1) 重置 `PATH` 为标准系统目录,使被劫持 `PATH`/`BASH_ENV` 的服务器无法
  替换 `awk`/`cat`/`df`/`sleep`;(2) 重组缓冲上限 1 MiB,防止只发数据不发同步标记
  的服务器耗尽内存;(3) 解析 `/proc` 与 `df` 输出改用饱和运算并对每次采样限 64 行,
  使构造的超大数值或伪造网卡洪流无法触发溢出 panic 或拖垮侧栏。

- **Sanitize remote file names before saving downloads (#26).** SFTP downloads
  built the local path straight from the server-supplied name, so a malicious
  server could use path separators, shell-special characters or a Windows
  reserved device name (`CON`, `NUL`, `COM1`…) to write outside the chosen
  folder or hit a device. Downloads now run the name through `sanitize_filename`
  (already used by the open/edit flow), which also gained reserved-device-name
  and leading-whitespace handling.
  **保存下载前清洗远程文件名 (#26)。** SFTP 下载直接用服务器给的文件名拼本地路径,
  恶意服务器可借路径分隔符、shell 特殊字符或 Windows 保留设备名(`CON`、`NUL`、
  `COM1`…)写到目标目录之外或命中设备。现在下载会先经 `sanitize_filename`
  (查看/编辑流程已在用)清洗,并新增了保留设备名与前导空白的处理。

- **Stop logging raw keystroke bytes (#15).** Debug logs recorded the hex of SSH
  input, which could include passwords; now they record only the byte length.
  A follow-up found two more leak sites in the key handler: `send_key` logged
  the raw key string (`key={:?}`) at debug level, and the `[KEY_DIAG]` IME
  diagnostic logged each Shift-typed key's code point at **info** level (no
  `RUST_LOG` needed) — both could expose password characters. They now go
  through a `redact_key` helper that reveals only C0/C1 control codes (what the
  IME diagnostics actually need) and masks every printable character.
  **不再记录原始按键字节 (#15)。** debug 日志原本记录 SSH 输入的十六进制(可能含
  密码),现在只记录字节长度。后续又发现按键处理里还有两处泄露:`send_key` 以
  debug 级打印按键原文(`key={:?}`),`[KEY_DIAG]` IME 诊断更是以 **info 级**
  (无需 `RUST_LOG`)打印每个带 Shift 按键的码位——都可能暴露密码字符。现在统一
  经 `redact_key` 处理,只保留 C0/C1 控制码(IME 诊断真正需要的),可打印字符一律掩码。

## [0.2.3] - 2026-06-05

### Added / 新增

- **Proxy support for SSH / SFTP (#7).** Connections can tunnel through a
  **SOCKS5** (`socks5://`) or **HTTP CONNECT** (`http://`) proxy, with optional
  `user:pass@` credentials. Set it per session in the dialog, or leave it blank
  to use the `$ALL_PROXY` environment variable; empty = direct.
  **SSH / SFTP 代理支持 (#7)。** 连接可经 **SOCKS5**(`socks5://`)或
  **HTTP CONNECT**(`http://`)代理(支持 `user:pass@` 认证)。会话对话框里按需
  填写,留空则用 `$ALL_PROXY` 环境变量,再空则直连。

- **Import hosts from `~/.ssh/config` (#1).** The "Import ~/.ssh/config" action
  (in the settings menu) parses the standard SSH config (`Host` / `HostName` /
  `User` / `Port` / `IdentityFile`, wildcard `Host *` blocks skipped) and adds
  each host as a session, skipping duplicates. Hosts with an `IdentityFile`
  default to key auth.
  **从 `~/.ssh/config` 导入主机 (#1)。** 设置菜单里的「导入 ~/.ssh/config」解析
  标准 SSH 配置(`Host` / `HostName` / `User` / `Port` / `IdentityFile`,跳过
  `Host *` 通配块),将每个主机加为会话并跳过重复;带 `IdentityFile` 的默认用密钥。

- **GitHub Actions release workflow** building native binaries for Windows /
  Linux / macOS (arm64 + x86_64) on each `v*` tag.
  **GitHub Actions 发布工作流**,每个 `v*` 标签自动构建 Windows / Linux /
  macOS(arm64 + x86_64)三平台二进制。

### Fixed / 修复

- The full-width `＋` before "New session" rendered as a tofu box in English;
  switched to an ASCII `+`.
  英文下「New session」前的全角 `＋` 显示为豆腐块,改用 ASCII `+`。

- `install-linux.sh` now auto-detects the `meatshell` binary sitting next to it
  in a release package, so it works with no arguments (it previously defaulted to
  the source-tree `./target/release` path and failed for end users).
  `install-linux.sh` 现在自动识别发布包里同目录的 `meatshell`,无需传参即可使用
  (之前默认指向源码树的 `./target/release`,普通用户直接跑会报错)。

## [0.2.2] - 2026-06-05

### Security / 安全

- **Fix Windows command injection (#12)** — `open_with_os` no longer shells out
  via `cmd /C start`; it calls `ShellExecuteW` directly so a malicious remote
  file name (e.g. `foo&calc.exe`) can't inject commands. Added `sanitize_filename`
  as defence-in-depth.
  **修复 Windows 命令注入 (#12)** —— 打开文件不再经 `cmd /C start`，改用
  `ShellExecuteW` 直接打开，恶意远程文件名（如 `foo&calc.exe`）无法注入命令；
  并新增 `sanitize_filename` 清洗作为纵深防御。

- **Stop echoing the saved password when editing a session (#10)** — the field
  is left blank with a "leave blank to keep" hint; an empty field on save keeps
  the existing password.
  **编辑会话时不再回显已保存密码 (#10)** —— 密码框留空并提示「留空则不修改」，
  保存时为空则保留原密码。

- **Zero passwords in memory on drop (#8)** — passwords now use a `Secret` type
  (`zeroize`) that wipes its heap buffer on drop and redacts itself in logs; the
  on-disk JSON format is unchanged.
  **密码内存清零 (#8)** —— 密码改用 `Secret` 类型（`zeroize`），Drop 时清零堆
  内存、日志中脱敏；磁盘 JSON 格式不变。

### Added / 新增

- **Internationalization — Chinese / English with runtime switching (#9).**
  Static UI uses Slint `@tr` + bundled `.po`; dynamic Rust strings use a `t()`
  helper. Switch via the gear menu; the choice is persisted and the default
  follows the system locale.
  **国际化 —— 中 / 英双语，运行时实时切换 (#9)。** 静态界面用 Slint `@tr` +
  bundled `.po`；Rust 动态文本用 `t()`。设置菜单里切换，选择会持久化，首次启动
  跟随系统语言。

- **Private-key file picker** in the session dialog, plus `.pub` fallback (auto
  strips the suffix to load the matching private key) and uniform `/` path
  separators across platforms.
  **会话弹窗的私钥文件选择器**，并支持 `.pub` 容错（自动去后缀加载对应私钥）、
  路径分隔符统一为 `/`。

- **Linux desktop integration** — `assets/meatshell.desktop` + `install-linux.sh`
  and an `xdg_app_id` so the GNOME/Ubuntu dock shows the app icon on Wayland.
  **Linux 桌面集成** —— `assets/meatshell.desktop` + `install-linux.sh`，并设置
  `xdg_app_id`，使 Wayland 下 GNOME/Ubuntu 任务栏显示应用图标。

- **Screenshots in the README** (`docs/screenshots/`, sensitive info redacted).
  **README 增加截图**（`docs/screenshots/`，敏感信息已打码）。

[0.3.8]: https://github.com/jeff141/meatshell/releases/tag/v0.3.8
[0.3.7]: https://github.com/jeff141/meatshell/releases/tag/v0.3.7
[0.3.3]: https://github.com/jeff141/meatshell/releases/tag/v0.3.3
[0.3.2]: https://github.com/jeff141/meatshell/releases/tag/v0.3.2
[0.3.1]: https://github.com/jeff141/meatshell/releases/tag/v0.3.1
[0.3.0]: https://github.com/jeff141/meatshell/releases/tag/v0.3.0
[0.2.2]: https://github.com/jeff141/meatshell/releases/tag/v0.2.2
