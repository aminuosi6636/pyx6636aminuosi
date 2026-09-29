# 本地视频素材库 · V1 架构设计

本设计先于代码完成。产品是 Tauri 2 桌面程序，React 只负责界面，Rust 是唯一能修改素材和数据库的入口。没有服务器、登录、云同步、Python 或运行时 Node.js。每位员工的数据隔离在各自系统用户的应用数据目录。阶段验收失败时停止进入下一阶段。

## 1. 最终目录结构

```text
video-library/
├─ docs/
│  ├─ ARCHITECTURE.md          架构、状态机、恢复决策
│  ├─ ACCEPTANCE.md            阶段验收与故障注入矩阵
│  └─ PHASE_STATUS.md          实测结果和未完成项
├─ src/
│  ├─ app/                    应用入口、启动状态、错误边界
│  ├─ components/             通用按钮、Modal、空状态
│  ├─ features/
│  │  ├─ setup/               首次选择目录、显式迁移目录
│  │  ├─ library/             分页查询、虚拟网格、卡片、统计
│  │  ├─ preview/             内置 video 播放、ESC、释放文件句柄
│  │  ├─ thumbnails/          video/canvas 队列、缓存、解码降级
│  │  └─ settings/            根目录、日志、备份状态
│  ├─ lib/                    类型化 IPC、错误翻译、请求锁
│  ├─ styles/                 视觉样式
│  └─ test/                   前端测试环境
├─ src-tauri/
│  ├─ capabilities/           仅主窗口所需权限
│  ├─ migrations/             不可修改的编号 SQL migrations
│  ├─ src/
│  │  ├─ lib.rs               Tauri 装配，保持薄层
│  │  ├─ commands/            IPC 输入验证与输出 DTO
│  │  ├─ domain/              命名、状态、错误、业务模型
│  │  ├─ services/            入库、使用次数、恢复、扫描、备份
│  │  ├─ db/                  连接、迁移、repository、查询
│  │  ├─ filesystem/          路径校验、文件身份、无覆盖改名
│  │  │  └─ platform/         Windows/macOS 原生适配
│  │  └─ infrastructure/     配置、日志、监控、后台任务
│  └─ tests/                  临时目录集成测试、故障注入
├─ scripts/                   环境诊断、schema 检查、性能 fixture
├─ .github/workflows/         两个操作系统分别测试与构建
├─ package.json / package-lock.json
├─ tsconfig*.json / vite.config.ts
└─ README.md
```

目录是最终目标；每阶段只创建实际用到的模块，不提前堆积空实现。Phase 1 不提供改名或入库按钮。

## 2. 模块职责与边界

| 模块 | 职责 | 约束 |
|---|---|---|
| React + TypeScript | 卡片、搜索/筛选/排序、虚拟网格、预览、缩略图采样、中文错误 | strict；不能直接修改路径、次数或 SQL；不乐观增加次数 |
| Tauri | 原生窗口、目录选择、单实例、类型化 IPC、受限本地媒体访问、安装包 | 主窗口权限最小化；不开放 shell 或任意文件系统写权限 |
| Rust domain/services | 命名、稳定性检查、扫描/监控、操作协调、恢复、备份、错误转换 | 所有素材变更走同一协调器；后台 I/O 不阻塞主线程 |
| Rust filesystem | PathBuf、文件身份、指纹、无覆盖改名、平台句柄 | 不删除素材，不用 copy+delete 模拟改名，不跟随符号链接越出授权目录 |
| Rust db | bundled SQLite、迁移、repository、索引、分页、事务 | 单写入任务队列；短读连接；所有 SQL 参数绑定 |
| SQLite | 素材记录、使用事件、可恢复操作日志、设置、序号、迁移版本 | WAL + synchronous=FULL + foreign_keys=ON；拒绝共享/网络数据库 |

数据库、日志、缩略图和备份在 Tauri 系统路径 API 返回的应用目录中，不放在安装目录或视频目录，不依赖工作目录。发布版本不打开开发 HTTP 服务。SQLite 静态编译到 Rust 程序中。

## 3. SQLite schema 与 migration

实际 schema 位于 `src-tauri/migrations/0001_initial.sql`。使用 UUID TEXT 主键；时间戳存 UTC RFC3339；`ingest_date` 为入库时系统当地日期 YYYY-MM-DD，之后不因时区或午夜变化而重写。

| 表 | 核心数据与用途 |
|---|---|
| libraries | UUID、根目录、目录身份；V1 同时只激活一个素材库；换目录先区分迁移原库与新建独立库 |
| videos | 指定的全部字段，另加 library_id、文件身份、mtime、原文件名、缩略图状态、最后扫描批次 |
| use_events | previous_count/new_count、used_at、operation_id；operation_id 唯一，恢复不能重复写事件 |
| operation_journal | 旧/新路径与次数、入库/使用操作类型、状态、指纹、request_id、错误；成功/回滚都保留 |
| settings | key/value；保存当前 library_id 和根目录设置，schema 版本以 PRAGMA user_version 为准 |
| daily_sequences | (library_id, ingest_date) 的 next_sequence，BEGIN IMMEDIATE 分配序号 |
| schema_migrations | 版本、迁移 SQL 的 BLAKE3 校验和、时间；迁移后不可修改已应用文件 |

状态约束：videos 为 ingesting / ready / missing / needs_review / error；操作为 pending / completed / rolled_back / needs_review。所有非终态操作对 video_id 建唯一部分索引，确保一个素材不能同时执行两次。指纹索引不唯一：内容相同的两个物理文件允许作为两条素材。

唯一约束 `(library_id, ingest_date, daily_sequence)`；每日序号至少三位，1000 后自然变为四位，不截断。分配后即便失败也不回收序号，避免复用已写入操作日志的目标文件名。正常三条连续入库仍为 001、002、003。

索引覆盖默认日期/序号排序、次数排序、日期过滤、指纹查找、未完成操作、事件时间。分页每页 100 条；优先 keyset pagination，排序组合包含 UUID 作为最后 tie-breaker。搜索日期/序号走结构化条件与索引；文件名子串查询使用参数化 LIKE，转义 %/_，200ms 延迟后查询，过期响应不覆盖最新查询结果。

统计区间不重叠：未使用=0，1–2 次，3–5 次，6 次及以上。用户的示例中“3–5”与“5 次以上”重叠，因此卡片统计最后一项明确写“6 次及以上”；筛选仍提供“5 次及以上”（>=5）。统计默认包括所有已正式入库记录，另显示文件丢失数量；未完成入库不计入。

迁移顺序：打开原文件 → quick_check → 检查支持的 user_version/校验和 → 配置连接 → 每个 migration 独立事务 → 写校验和和 user_version → commit。已有库升级前先用 SQLite Backup API 备份；Phase 1 仅首次迁移。数据库损坏、版本较新、迁移校验和改变时进入停止写入状态，绝不删除或自动重建数据库。禁止用直接复制 `.sqlite` 文件备份 WAL 数据库。

## 4. 新素材入库状态机

```text
发现候选 → 等待稳定 → 校验身份/内容 → 分配日期和序号并写 pending
    ↑          │                   │
文件变化 ←─────┘                   ↓
                              无覆盖原生改名
                                   ↓
                   提交素材 ready + operation completed
                                   ↓
                       事件通知 UI → 异步生成缩略图
```

1. 首次启动、手动扫描、监控事件都进入同一队列。递归扫描授权根目录的普通文件，支持扩展名大小写不敏感的 mp4/mov/m4v/avi/mkv。跳过符号链接、临时扩展名及应用目录。watcher 事件可能重复、丢失、乱序，只作为扫描提示；每隔 60 秒轻量补扫，完整扫描用于对账。
2. 每 500ms 采样文件大小、mtime、身份；连续至少 2 秒均不变才进入校验；大小=0 继续等候。读取期间或改名前任一值变化，回到等待稳定。文件仍忙时退避重试，不强行移动。
3. 流式 BLAKE3 全文件指纹，不整段读进内存；最多两个后台读任务。记录磁盘卷/文件 ID，hash 前后复查元数据。仅凭文件大小不证明复制结束；macOS 不能保证其他非协作进程不会再次写入，采样窗口是启发式条件，发现再次修改立即停止变更并标记复核。
4. 单写入队列在 BEGIN IMMEDIATE 中读取此刻当地日期、分配序号、插入 ingesting 素材和 pending 操作，再提交日志。目标在原文件同一个父目录，保留子目录结构和原扩展名。不依据已有命名推断入库日期/使用次数。
5. 原生“目标存在则失败”的改名成功后，SQLite 单事务把视频改为 ready 和操作 completed。若提交失败，按第 5 节决策补偿。缩略图/时长失败不会撤销成功入库。
6. 正在 ingesting 的素材不展示为可用；错误提供原路径及重试入口。同名冲突停止该操作，保留两个文件原样，不能“先删除目标”或擅自覆盖。

手动改名识别顺序：路径仍在且身份匹配 → 文件 ID 匹配并核验内容 → 缺失记录中唯一的完整指纹/大小候选。重复内容、多个缺失候选、内容变动时都进入人工确认。fingerprint 不是唯一 ID。不在扫描失败或根目录暂时不可用时批量标记 missing；只有成功完整遍历后的对账才判断缺失。

## 5. “使用一次”安全事务

文件系统和 SQLite 无法共享一个原子事务，采用 write-ahead journal + 补偿 + 启动对账。SQLite commit 成功与否不明确时，先重新读取操作记录再决定，不盲目回滚。

1. UI 点击瞬间取得以 video UUID 为键的同步请求锁，禁用按钮。IPC 携带随机 request_id 与 expected_count；先释放该视频的 preview 和缩略图 video.src/句柄。
2. Rust 协调器用 try-lock 拒绝同素材并发请求，数据库唯一非终态操作兜底；重复 request_id 返回原结果，过期 expected_count 返回“素材已更新，请刷新”，不能排队累计。点击锁覆盖改名和整个数据库提交，finally 才释放。
3. 校验 ready、根目录授权、文件存在、身份与指纹一致、计数未溢出。计数不信任 UI 或文件名。
4. SQLite 独立短事务持久化 pending（old/new 路径、old/new count、指纹和请求 ID），提交并同步。日志提交失败绝不改名。
5. 平台 adapter 原子无覆盖改名，同父目录、同卷，复查文件身份。目标占用/权限不足/冲突时 count 不变；操作 rolled_back 或无法确认时 needs_review。
6. SQLite 单事务使用 CAS `WHERE id=? AND use_count=? AND current_path=?` 更新 path/filename/count，插入唯一 use_event，标 completed；三项必须一起 commit。
7. definite rollback 且数据库仍旧值：核验目标身份后尝试无覆盖反向改名，成功才标 rolled_back。旧路径已被其他文件占据时不能覆盖，保留 pending/needs_review 并停止该视频后续操作。commit 结果不明时重新打开数据库读日志；确认 completed 就返回成功，无法读库则保持恢复状态。
8. 返回提交后的视频 DTO。UI 使用返回值刷新卡片和统计，不预先 +1。若 IPC 回应丢失，下次以相同 request_id 查询结果，不能当作新的点击。

本方案不承诺坏盘/任意外部修改条件下绝对原子性；承诺不覆盖/删除素材、可解释的停止状态、恢复过程中不重复增加次数。

## 6. 启动时异常恢复

顺序：单实例 → 初始化日志/系统目录 → 打开并检查数据库 → migration → 核验根目录 → 恢复非终态日志 → 完整扫描对账 → 当日数据库备份 → 启用监控 → 进入可操作界面。恢复期间禁用素材变更。

对每条 pending 用完整身份/指纹核验路径，不能只看文件是否存在：

| 文件状态 | 数据库状态 | 自动动作 |
|---|---|---|
| 只有 old，且身份一致 | 旧记录/ingesting | 标 rolled_back；入库保留 error 记录供重试，不删视频 |
| 只有 new，且身份一致 | 旧记录/ingesting | 向前完成，CAS 更新、唯一事件、completed 一次提交 |
| 只有 new，且身份一致 | 已完成记录 | 核验 count/path/event 后幂等完成，无重复事件 |
| old/new 都存在 | 任意 | needs_review，停止自动改名，不猜测、不覆盖 |
| 都不存在 | 任意 | 标 missing + needs_review，保留历史记录与操作 |
| 指纹/身份不符 | 任意 | needs_review，保留现场，显示人工处理入口 |
| 根目录离线、权限不足、数据库不可读 | 任意 | 暂停恢复和监控；修复路径/权限后重试，不能批量报丢失 |

恢复动作本身可反复被中断，每次重启使用同一决策表和唯一 operation_id 事件约束。已回滚操作不自动重新执行；用户重试创建新 request_id。

备份使用 SQLite Online Backup API 写入应用 backup 目录临时文件，完成后 quick_check，再无覆盖发布当天备份名；一天最多一份成功备份，保留最近 7 份，仅清理应用生成的旧数据库备份。备份失败记录提示，不假称已备份。不复制素材。

## 7. Windows/macOS 差异

| 事项 | Windows | macOS | 共同处理 |
|---|---|---|---|
| 改名 | 带 DELETE 权限文件句柄 + SetFileInformationByHandle，ReplaceIfExists=false；不开放 share-write，忙文件失败 | renameatx_np/renamex_np 的 RENAME_EXCL，目标存在则失败；父目录 FD 限定位置 | 不使用可能替换目标的 std::fs::rename；不支持安全原语的文件系统停止操作 |
| 占用 | 剪辑软件不共享 delete 时会阻止改名 | 打开的文件通常仍可改名，外部编辑器继续持有旧句柄 | 改名前释放本应用媒体句柄；中文提示关闭相关程序；不强行解锁 |
| 路径 | UTF-16、反斜线、大小写通常不敏感、保留名、长路径 | 路径分隔符 /，卷大小写策略不同，Unicode 可能分解 | PathBuf/OsStr；不用字符串替换分隔符；不把 lowercased path 当 ID |
| 文件身份 | Volume ID + File ID | device + inode | 身份会复用，恢复须结合完整内容指纹；不跨卷移动 |
| 权限 | ACL、只读属性、Controlled Folder Access | POSIX 权限、TCC 对 Documents/外置盘访问限制 | 目录选择原生 dialog；权限不足停止变更，不修改系统权限 |
| 播放 | WebView2 支持由系统和编码决定 | WKWebView 支持由系统和编码决定 | mp4 扩展名也不保证编码可播；fallback 图标/中文解码提示，duration 可空 |
| 打包 | MSVC + NSIS setup.exe，安装器离线提供 WebView2 | Apple Silicon runner + aarch64-apple-darwin，app/dmg | CI 在各自系统构建；开发机需要工具链，员工机器不需要 |
| 分发 | 建议代码签名，未签名会影响系统信任提示 | 正式员工分发需 Developer ID 签名与 notarization | 签名凭据只用 CI secrets；无凭据的测试构建不冒充正式发布 |

V1 不支持网络共享素材根目录，不接受越界符号链接。移动整库通过显式“重新定位原素材库”确认根身份，不能把目录不存在当作空库。媒体访问动态限定根目录与缓存目录；不设置全盘 asset scope。

## 8. MVP 实施顺序和退出条件

| 阶段 | 实施顺序 | 必须通过后才能继续 |
|---|---|---|
| 1 骨架 | Tauri/React/strict TS、SQLite bundled、migration、中文错误 DTO、日志、单实例、启动诊断 | 实际 tauri dev 开窗；前端构建/测试；Rust test/check；重复迁移和故障保护 |
| 2 目录 | 原生选择/保存根目录、后台扫描、稳定队列、指纹、数据库记录、路径校验 | 重启记忆；复制中文视频；复制途中不处理；权限/目录离线不乱标丢失 |
| 3 命名 | 日期/序号分配、无覆盖 adapter、入库 journal、恢复原语 | 001/002/跨日001；冲突/占用不覆盖；崩溃点恢复 |
| 4 界面 | 分页/虚拟网格、卡片、搜索/筛选/排序、预览、缩略图缓存 | 默认003→002→001；ESC与播放控制；损坏/不支持编码降级 |
| 5 使用 | 后端锁/CAS/request_id、日志、use_events、改名+数据库提交、UI 锁 | 0→1、3→4；5个并发请求仅1次；文件/数据库同步；失败不误增 |
| 6 可靠性 | 完整恢复、周期监控/对账、备份、丢失/改名识别、日志入口 | 各崩溃点幂等恢复；不确定映射复核；7份备份完整；数据库损坏无素材操作 |
| 7 性能 | 1000/5000/10000数据库和文件 fixture、分页指标、缓存并发控制 | 记录机器/样本/耗时/DOM数；搜索p95目标<200ms（不含200ms输入延迟）；首屏目标<1s；挂载卡片<100 |
| 8 打包 | Windows/macOS matrix、安装/卸载、签名、干净机器人工验收 | 双系统安装后无工具链可开；离线启动；中文根目录；真实视频验收流程 |

为了保证 Phase 2 的扫描不会留下危险临时代码：Phase 2 只建立候选/记录，不自动改名；Phase 3 才开放正式入库。operation journal 的入库分支必须在第一次真实改名前落地，不能等 Phase 6 才补上。

## 官方技术依据

- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri Windows installer/WebView2](https://v2.tauri.app/distribute/windows-installer/)
- [Rust fs::rename 的覆盖行为](https://doc.rust-lang.org/std/fs/fn.rename.html)
- [SQLite synchronous 与 WAL](https://sqlite.org/pragma.html#pragma_synchronous)
- [SQLite Online Backup API](https://www.sqlite.org/backup.html)
- [Apple APFS 的 RENAME_EXCL](https://devstreaming-cdn.apple.com/videos/wwdc/2017/715gk347h3udl/715/715_whats_new_in_apple_file_system.pdf)
