//! 双语机制（中 / 英）：**单一 message catalog** + 取词 API + 语言来源。
//!
//! 用户 2026-09-27 定案（分阶段：本阶段只立骨架 + 打通一条竖切，不做全量翻译）：
//! * 卡定义只有一份，**卡面标签随语言渲染**（禁止给每张卡再复制一份英文卡）；
//!   标准符号（`D_ei` / `Az` / `Wn` / `Kn` / `d_B` …）保持原样不译。
//! * 单一 catalog（表驱动，**一行一 key**：`zh` / `en`），按域分节
//!   （命令输出 / 报错 / GUI / 卡面标签 / 计算书 / 手册）。
//! * 语言 = **跟随宿主 + 可手动覆盖**：宿主 API（`ocs_plugin_api`）当前**没有**
//!   locale/language 字段（已核对 HostApi 全部方法），因此用环境变量兜底 + 程序内
//!   覆盖；一旦宿主暴露语言设置，只需改 [`resolve_env_lang`] 一处。
//!
//! # 语言来源（优先级从高到低）
//!
//! 1. 程序内 [`set_lang`]（GUI 设置 / 命令开关 / 测试；可 [`set_lang_auto`] 复位）；
//! 2. 环境变量 [`LANG_ENV`]（`OCSMLANG=zh|en`，用户/运维手动覆盖）；
//! 3. 别名 [`LANG_ENV_ALIAS`]（`OCSM_LANG`）；
//! 4. `LC_ALL` → `LC_MESSAGES` → `LANG`（宿主/system locale 的代理）；
//! 5. 默认 **`zh`**（保守默认：本仓既有输出全是中文，缺省不改变现行为）。
//!
//! 值接受 `zh` / `zh-CN` / `zh_CN.UTF-8` / `en` / `en_US.UTF-8`（大小写不敏感，
//! 只看 `_` `-` `.` `@` 前的第一段）。
//!
//! # 缺 key 的保守默认（**绝不返回静默空串**）
//!
//! * key 在 catalog 里、目标语言串为空 → **回落 `zh`**，并记入 [`missing_keys`]；
//! * key 完全不在 catalog 里 → 返回 **key 本身**（可见占位，便于一眼定位），
//!   并记入 [`missing_keys`]；
//! * 运行期不 panic（插件里让命令整个挂掉更糟）；缺失由 [`catalog_problems`]
//!   静态测试 + [`missing_keys`] 运行期点名兜住。
//!
//! 其它：`{name}` 占位符用 [`t_fmt`] 插值；catalog 完整性（两语齐全 / 无重复 key /
//! 占位符两语一致）由 [`catalog_problems`] 覆盖，见本文件底部单测。
//!
// 阶段 1 骨架：语言切换/诊断/缺 key 点名等 API 是给第二阶段接线与联调用的，
// 产品路径目前只用了 t/t_fmt；先允许 dead_code，不给构建加新警告。
#![allow(dead_code)]

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

/// 已支持的语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    /// 稳定语言代号（下发 GUI / 写日志用）。
    pub fn code(self) -> &'static str {
        match self {
            Lang::Zh => "zh",
            Lang::En => "en",
        }
    }

    /// 人类侧显示名（本身也走 catalog，随当前语言）。
    pub fn label(self) -> String {
        t(match self {
            Lang::Zh => "meta.lang.zh",
            Lang::En => "meta.lang.en",
        })
    }
}

/// 一条词条：**一行一 key：`zh` / `en`**。
#[derive(Debug, Clone, Copy)]
pub struct Msg {
    /// 稳定 key（`域.对象.用途`，如 `cmd.tf.usage`）。
    pub key: &'static str,
    /// 中文（缺省语言；本阶段字段名照「先立骨架」口径保留）。
    pub zh: &'static str,
    /// 英文。
    pub en: &'static str,
}

impl Msg {
    const fn new(key: &'static str, zh: &'static str, en: &'static str) -> Self {
        Self { key, zh, en }
    }
}

// ───────────────────────── catalog（按域分节） ─────────────────────────
//
// 本阶段只登记「已接线」的词条（竖切域）与语言元信息；其余全量字符串先盘点
// （`~/桌面/OCSM/review/i18n_盘点.md`），按域分批录入，**列在这里的必须真有人用**。

/// 单一 message catalog。加词条 = 按域在对应小节加一行。
pub const CATALOG: &[Msg] = &[
    // ── 语言元信息（GUI / 测试）──────────────────────────────────────
    Msg::new("meta.lang.zh", "中文", "Chinese"),
    Msg::new("meta.lang.en", "英文", "English"),

    // ── 命令输出（命令回执 / 用法 / 提示）────────────────────────────
    // 竖切域 ①：`TF` 的用法串（见 lib.rs::parse_frame_args）。
    Msg::new(
        "cmd.tf.usage",
        "用法：TF <名称> [比例] [at x,y] [rot 度]（不带参数 = 打开选择窗口）",
        "Usage: TF <name> [scale] [at x,y] [rot deg] (no args = open the picker window)",
    ),

    // ── 报错（解析错误 / 校验失败）──────────────────────────────────
    // 竖切域 ②：`TF` 参数认不出（带 `{arg}` / `{usage}` 插值，见 lib.rs）。
    Msg::new(
        "cmd.tf.err.unknown_param",
        "认不出的参数「{arg}」。{usage}",
        "Unrecognized argument \"{arg}\". {usage}",
    ),
];

/// 手动覆盖：环境变量 `OCSMLANG`（最高优先级的进程外开关）。
pub const LANG_ENV: &str = "OCSMLANG";
/// 手动覆盖：`OCSM_LANG`（兼容别名）。
pub const LANG_ENV_ALIAS: &str = "OCSM_LANG";

// 0 = 自动（跟随环境），1 = zh，2 = en。
const LANG_AUTO: u8 = 0;
const LANG_ZH: u8 = 1;
const LANG_EN: u8 = 2;

static OVERRIDE: AtomicU8 = AtomicU8::new(LANG_AUTO);
/// 环境解析结果缓存（`OnceLock` 不能复位，测试要复位 → 用 `Mutex<Option<_>>`）。
static AUTO_CACHE: Mutex<Option<Lang>> = Mutex::new(None);
/// 运行期缺 key 点名表（有上限，避免异常路径把内存撑爆）。
static MISSING: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// 缺 key 记录上限。
const MISSING_LIMIT: usize = 256;

/// 按 key 查词条（线性扫描；catalog 到几百条前够用，变大再换表）。
pub fn lookup(key: &str) -> Option<&'static Msg> {
    CATALOG.iter().find(|m| m.key == key)
}

/// 当前语言（解析见模块头注释）。
pub fn lang() -> Lang {
    match OVERRIDE.load(Ordering::Relaxed) {
        LANG_ZH => Lang::Zh,
        LANG_EN => Lang::En,
        _ => {
            let mut cache = AUTO_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            *cache.get_or_insert_with(resolve_env_lang)
        }
    }
}

/// 手动覆盖当前语言（GUI 设置 / 命令开关 / 测试）。
pub fn set_lang(lang: Lang) {
    OVERRIDE.store(
        match lang {
            Lang::Zh => LANG_ZH,
            Lang::En => LANG_EN,
        },
        Ordering::Relaxed,
    );
}

/// 复位为「自动」（跟随环境；顺带丢掉环境解析缓存，避免拿到旧值）。
pub fn set_lang_auto() {
    OVERRIDE.store(LANG_AUTO, Ordering::Relaxed);
    reset_auto_cache();
}

/// 复位环境解析缓存（进程内改了 `LANG*` / `OCSMLANG` 后想立刻生效时调用；
/// 正常部署只在进程启动时解析一次）。
pub fn reset_auto_cache() {
    *AUTO_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// 解析一个语言值（`zh-CN` / `en_US.UTF-8` / `C` …）；不认识的返回 `None`。
pub fn parse_lang(raw: &str) -> Option<Lang> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    let head = s.split(['_', '-', '.', '@']).next().unwrap_or("");
    match head {
        "zh" | "cn" | "chinese" => Some(Lang::Zh),
        "en" | "english" => Some(Lang::En),
        _ => None,
    }
}

/// 环境变量 → 语言（含手动覆盖 `OCSMLANG`；见模块头优先级）。默认 `zh`。
pub fn resolve_env_lang() -> Lang {
    for var in [LANG_ENV, LANG_ENV_ALIAS, "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if let Some(l) = parse_lang(&v) {
                return l;
            }
        }
    }
    Lang::Zh
}

/// 语言来源诊断（GUI / 日志）：返回 `(变量名, 原值)`；全未命中 = `None`。
pub fn env_lang_source() -> Option<(&'static str, String)> {
    for var in [LANG_ENV, LANG_ENV_ALIAS, "LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if parse_lang(&v).is_some() {
                return Some((var, v));
            }
        }
    }
    None
}

fn note_missing(key: &str) {
    let mut set = MISSING.lock().unwrap_or_else(|e| e.into_inner());
    if set.len() < MISSING_LIMIT && !set.iter().any(|k| k == key) {
        set.push(key.to_string());
    }
}

/// 渲染一条词条（空串 → 回落 `zh`；`zh` 也空 → `None`）。单测直接喂构造词条。
fn render_msg(m: &Msg, lang: Lang) -> Option<&'static str> {
    let pick = match lang {
        Lang::Zh => m.zh,
        Lang::En => m.en,
    };
    if !pick.is_empty() {
        Some(pick)
    } else if !m.zh.is_empty() {
        Some(m.zh)
    } else {
        None
    }
}

/// 取词（按当前语言；返回 owned `String`，避免把 catalog 生命周期漏到调用方）。
pub fn t(key: &str) -> String {
    t_lang(lang(), key)
}

/// 取词（指定语言；测试 / GUI 预渲染用）。
pub fn t_lang(lang: Lang, key: &str) -> String {
    match lookup(key) {
        Some(m) => match render_msg(m, lang) {
            Some(s) => s.to_string(),
            // catalog 里 zh 也是空（不该发生；catalog_problems 会点名）→ 可见 key。
            None => {
                note_missing(key);
                key.to_string()
            }
        },
        None => {
            note_missing(key);
            // 保守默认：不返回空串 / 不 panic；key 本身可见，便于一眼定位。
            key.to_string()
        }
    }
}

/// 带参数插值：`{name}` → `args` 里的值（未给到的占位符原样保留）。
pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String {
    t_fmt_lang(lang(), key, args)
}

/// 带参数插值（指定语言）。
pub fn t_fmt_lang(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    let mut out = t_lang(lang, key);
    for (k, v) in args {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// 运行期缺失 key 名单（去重点名；有上限）。测试 / 联调用。
pub fn missing_keys() -> Vec<String> {
    MISSING.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// 清空缺失记录。
pub fn clear_missing_keys() {
    MISSING.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// catalog 静态问题清单：空 key / 重复 key / 任一语为空 / `{占位符}` 两语不一致。
/// 空 = 健康。
pub fn catalog_problems() -> Vec<String> {
    let mut problems = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for m in CATALOG {
        if m.key.trim().is_empty() {
            problems.push("空 key".to_string());
        }
        if seen.contains(&m.key) {
            problems.push(format!("重复 key：{}", m.key));
        } else {
            seen.push(m.key);
        }
        if m.zh.trim().is_empty() {
            problems.push(format!("{}：zh 为空", m.key));
        }
        if m.en.trim().is_empty() {
            problems.push(format!("{}：en 为空", m.key));
        }
        if placeholders(m.zh) != placeholders(m.en) {
            problems.push(format!("{}：占位符 zh/en 不一致", m.key));
        }
    }
    problems
}

/// 提取 `{name}` 占位符名（排序去重）。
fn placeholders(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        let name = &after[..end];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用例之间语言/环境是进程级全局 —— 与仓内各模块共用同一把测试锁。
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::global_state_test_lock()
    }

    #[test]
    fn catalog_is_complete_and_has_no_duplicate_keys() {
        let problems = catalog_problems();
        assert!(problems.is_empty(), "catalog 静态问题：{problems:?}");
        assert!(!CATALOG.is_empty(), "catalog 不该为空（至少竖切域词条）");
    }

    #[test]
    fn placeholder_extraction_handles_nested_and_bad_forms() {
        assert_eq!(placeholders("a {x} b {y} {x}"), vec!["x", "y"]);
        assert_eq!(placeholders("无占位符"), Vec::<String>::new());
        assert_eq!(placeholders("残缺 {x"), Vec::<String>::new());
        assert_eq!(placeholders("{}"), Vec::<String>::new());
    }

    #[test]
    fn t_switches_between_zh_and_en() {
        let _g = lock();
        set_lang(Lang::Zh);
        let zh = t("cmd.tf.usage");
        assert!(zh.starts_with("用法：TF <名称>"), "中文取词：{zh}");
        set_lang(Lang::En);
        let en = t("cmd.tf.usage");
        assert!(en.starts_with("Usage: TF <name>"), "英文取词：{en}");
        assert_ne!(zh, en);
        set_lang_auto();
    }

    #[test]
    fn t_fmt_interpolates_in_both_languages() {
        let _g = lock();
        let arg = "extra";
        let usage = "U";
        let zh = t_fmt_lang(
            Lang::Zh,
            "cmd.tf.err.unknown_param",
            &[("arg", arg), ("usage", usage)],
        );
        assert_eq!(zh, "认不出的参数「extra」。U");
        let en = t_fmt_lang(
            Lang::En,
            "cmd.tf.err.unknown_param",
            &[("arg", arg), ("usage", usage)],
        );
        assert_eq!(en, "Unrecognized argument \"extra\". U");
    }

    #[test]
    fn missing_en_falls_back_to_zh_and_is_named() {
        let _g = lock();
        // 合成词条：en 空 → 回落 zh（真实 catalog 由完整性测试保证不为空）。
        let m = Msg::new("synthetic.only.zh", "只有中文", "");
        assert_eq!(render_msg(&m, Lang::En), Some("只有中文"));
        assert_eq!(render_msg(&m, Lang::Zh), Some("只有中文"));
        // zh / en 都空 → None（调用方转成「可见 key」，不是空串）。
        let empty = Msg::new("synthetic.empty", "", "");
        assert_eq!(render_msg(&empty, Lang::En), None);

        // catalog 里根本没有的 key：返回 key 本身 + 运行期点名，绝不空串。
        clear_missing_keys();
        let got = t_lang(Lang::En, "no.such.key.at.all");
        assert_eq!(got, "no.such.key.at.all");
        assert!(missing_keys().iter().any(|k| k == "no.such.key.at.all"));
        clear_missing_keys();
    }

    #[test]
    fn env_resolution_prefers_ocsmlang_then_locale() {
        let _g = lock();
        // 环境变量是进程级全局：改前存旧值，测完恢复。
        let vars = [LANG_ENV, LANG_ENV_ALIAS, "LC_ALL", "LC_MESSAGES", "LANG"];
        let saved: Vec<(&str, Option<String>)> =
            vars.iter().map(|v| (*v, std::env::var(v).ok())).collect();
        for v in vars {
            std::env::remove_var(v);
        }
        assert_eq!(resolve_env_lang(), Lang::Zh, "全无环境 → 保守默认 zh");
        std::env::set_var("LANG", "en_US.UTF-8");
        assert_eq!(resolve_env_lang(), Lang::En, "跟随 LANG");
        std::env::set_var("LC_ALL", "zh_CN.UTF-8");
        assert_eq!(resolve_env_lang(), Lang::Zh, "LC_ALL 优先于 LANG");
        std::env::set_var(LANG_ENV, "en");
        assert_eq!(resolve_env_lang(), Lang::En, "OCSMLANG 是手动覆盖（最高）");
        std::env::set_var(LANG_ENV, "de_DE");
        assert_eq!(
            resolve_env_lang(),
            Lang::Zh,
            "不认识的值跳过，看下一来源（LC_ALL=zh 生效）"
        );
        std::env::set_var("LC_ALL", "de_DE");
        assert_eq!(resolve_env_lang(), Lang::En, "LC_ALL 也不认识 → 落到 LANG=en");
        assert_eq!(parse_lang("zh-CN"), Some(Lang::Zh));
        assert_eq!(parse_lang("EN_us"), Some(Lang::En));
        assert_eq!(parse_lang("C"), None);
        assert_eq!(parse_lang("  "), None);
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
    }
}
