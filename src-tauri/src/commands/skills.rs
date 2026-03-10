use crate::utils::openclaw_command_async;
use serde_json::Value;

#[cfg(target_os = "windows")]
#[allow(unused_imports)]
use std::os::windows::process::CommandExt;

/// 列出所有 Skills 及其状态（openclaw skills list --json）
#[tauri::command]
pub async fn skills_list() -> Result<Value, String> {
    let output = openclaw_command_async()
        .args(["skills", "list", "--json", "--verbose"])
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            serde_json::from_str(&stdout).map_err(|e| format!("解析失败: {e}"))
        }
        _ => {
            // CLI 不可用时，兜底扫描本地 skills 目录
            scan_local_skills()
        }
    }
}

/// 查看单个 Skill 详情（openclaw skills info <name> --json）
#[tauri::command]
pub async fn skills_info(name: String) -> Result<Value, String> {
    let output = openclaw_command_async()
        .args(["skills", "info", &name, "--json"])
        .output()
        .await
        .map_err(|e| format!("执行 openclaw 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("获取详情失败: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| format!("解析详情失败: {e}"))
}

/// 检查 Skills 依赖状态（openclaw skills check --json）
#[tauri::command]
pub async fn skills_check() -> Result<Value, String> {
    let output = openclaw_command_async()
        .args(["skills", "check", "--json"])
        .output()
        .await
        .map_err(|e| format!("执行 openclaw 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("检查失败: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&stdout).map_err(|e| format!("解析失败: {e}"))
}

/// 安装 Skill 依赖（根据 install spec 执行 brew/npm/go/uv/download）
#[tauri::command]
pub async fn skills_install_dep(kind: String, spec: Value) -> Result<Value, String> {
    let path_env = super::enhanced_path();

    let (program, args) = match kind.as_str() {
        "brew" => {
            let formula = spec
                .get("formula")
                .and_then(|v| v.as_str())
                .ok_or("缺少 formula 参数")?
                .to_string();
            ("brew".to_string(), vec!["install".to_string(), formula])
        }
        "node" => {
            let package = spec
                .get("package")
                .and_then(|v| v.as_str())
                .ok_or("缺少 package 参数")?
                .to_string();
            (
                "npm".to_string(),
                vec!["install".to_string(), "-g".to_string(), package],
            )
        }
        "go" => {
            let module = spec
                .get("module")
                .and_then(|v| v.as_str())
                .ok_or("缺少 module 参数")?
                .to_string();
            ("go".to_string(), vec!["install".to_string(), module])
        }
        "uv" => {
            let package = spec
                .get("package")
                .and_then(|v| v.as_str())
                .ok_or("缺少 package 参数")?
                .to_string();
            (
                "uv".to_string(),
                vec!["tool".to_string(), "install".to_string(), package],
            )
        }
        other => return Err(format!("不支持的安装类型: {other}")),
    };

    let mut cmd = tokio::process::Command::new(&program);
    cmd.args(&args).env("PATH", &path_env);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("执行 {program} 失败: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!(
            "安装失败 ({program} {}): {}",
            output.status,
            stderr.trim()
        ));
    }

    Ok(serde_json::json!({
        "success": true,
        "output": stdout.trim(),
    }))
}

/// 从 ClawHub 安装 Skill（npx clawhub install <slug>）
#[tauri::command]
pub async fn skills_clawhub_install(slug: String) -> Result<Value, String> {
    let path_env = super::enhanced_path();
    let home = dirs::home_dir().unwrap_or_default();

    // 确保 skills 目录存在
    let skills_dir = super::openclaw_dir().join("skills");
    if !skills_dir.exists() {
        std::fs::create_dir_all(&skills_dir).map_err(|e| format!("创建 skills 目录失败: {e}"))?;
    }

    let mut cmd = tokio::process::Command::new("npx");
    cmd.args(["-y", "clawhub", "install", &slug])
        .env("PATH", &path_env)
        .current_dir(&home);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("执行 clawhub 失败: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(format!("安装失败: {}", stderr.trim()));
    }

    Ok(serde_json::json!({
        "success": true,
        "slug": slug,
        "output": stdout.trim(),
    }))
}

/// 从 ClawHub 搜索 Skills（npx clawhub search <query>）
#[tauri::command]
pub async fn skills_clawhub_search(query: String) -> Result<Value, String> {
    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(Value::Array(vec![]));
    }

    let path_env = super::enhanced_path();
    let mut cmd = tokio::process::Command::new("npx");
    cmd.args(["-y", "clawhub", "search", &q])
        .env("PATH", &path_env);
    #[cfg(target_os = "windows")]
    cmd.creation_flags(0x08000000);
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("执行 clawhub 失败: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("搜索失败: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // clawhub search 输出是文本行，每行一个 skill
    let items: Vec<Value> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('-') && !l.starts_with("Search"))
        .map(|l| {
            let parts: Vec<&str> = l.splitn(2, char::is_whitespace).collect();
            let slug = parts.first().unwrap_or(&"").trim();
            let desc = parts.get(1).unwrap_or(&"").trim();
            serde_json::json!({
                "slug": slug,
                "description": desc,
                "source": "clawhub"
            })
        })
        .filter(|v| !v["slug"].as_str().unwrap_or("").is_empty())
        .collect();

    Ok(Value::Array(items))
}

/// CLI 不可用时的兜底：扫描 ~/.openclaw/skills 目录
fn scan_local_skills() -> Result<Value, String> {
    let skills_dir = super::openclaw_dir().join("skills");
    if !skills_dir.exists() {
        return Ok(serde_json::json!({
            "skills": [],
            "source": "local-scan",
            "cliAvailable": false
        }));
    }

    let mut skills = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&skills_dir) {
        for entry in entries.flatten() {
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if !ft.is_dir() && !ft.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let skill_md = entry.path().join("SKILL.md");
            let description = if skill_md.exists() {
                // 尝试从 SKILL.md 的 frontmatter 中提取 description
                parse_skill_description(&skill_md)
            } else {
                String::new()
            };
            skills.push(serde_json::json!({
                "name": name,
                "description": description,
                "source": "managed",
                "eligible": true,
                "bundled": false,
                "filePath": skill_md.to_string_lossy(),
            }));
        }
    }

    Ok(serde_json::json!({
        "skills": skills,
        "source": "local-scan",
        "cliAvailable": false
    }))
}

/// 从 SKILL.md 的 YAML frontmatter 中提取 description
fn parse_skill_description(path: &std::path::Path) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    // frontmatter 格式: ---\n...\n---
    if !content.starts_with("---") {
        return String::new();
    }
    if let Some(end) = content[3..].find("---") {
        let fm = &content[3..3 + end];
        for line in fm.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("description:") {
                return rest.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }
    String::new()
}
