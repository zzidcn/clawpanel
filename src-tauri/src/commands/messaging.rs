/// 消息渠道管理
/// 负责 Telegram / Discord / QQ Bot 等消息渠道的配置持久化与凭证校验
/// 配置写入 openclaw.json 的 channels / plugins 节点
use serde_json::{json, Map, Value};

/// 读取指定平台的当前配置（从 openclaw.json 中提取表单可用的值）
#[tauri::command]
pub async fn read_platform_config(platform: String) -> Result<Value, String> {
    let cfg = super::config::load_openclaw_json()?;

    // 从已有配置中提取用户可编辑字段
    let saved = cfg
        .get("channels")
        .and_then(|c| c.get(&platform))
        .cloned()
        .unwrap_or(Value::Null);

    if saved.is_null() {
        return Ok(json!({ "exists": false }));
    }

    let mut form = Map::new();

    match platform.as_str() {
        "discord" => {
            // Discord 配置在 openclaw.json 中是展开的 guilds 结构
            // 需要反向提取成表单字段：token, guildId, channelId
            if let Some(t) = saved.get("token").and_then(|v| v.as_str()) {
                form.insert("token".into(), Value::String(t.into()));
            }
            if let Some(guilds) = saved.get("guilds").and_then(|v| v.as_object()) {
                if let Some(gid) = guilds.keys().next() {
                    form.insert("guildId".into(), Value::String(gid.clone()));
                    if let Some(channels) = guilds[gid].get("channels").and_then(|v| v.as_object())
                    {
                        let cids: Vec<&String> =
                            channels.keys().filter(|k| k.as_str() != "*").collect();
                        if let Some(cid) = cids.first() {
                            form.insert("channelId".into(), Value::String((*cid).clone()));
                        }
                    }
                }
            }
        }
        "telegram" => {
            // Telegram: botToken 直接保存, allowFrom 数组需要拼回逗号字符串
            if let Some(t) = saved.get("botToken").and_then(|v| v.as_str()) {
                form.insert("botToken".into(), Value::String(t.into()));
            }
            if let Some(arr) = saved.get("allowFrom").and_then(|v| v.as_array()) {
                let users: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                form.insert("allowedUsers".into(), Value::String(users.join(", ")));
            }
        }
        "qqbot" => {
            // QQ Bot: token 格式为 "AppID:AppSecret"，拆分回表单字段
            if let Some(t) = saved.get("token").and_then(|v| v.as_str()) {
                if let Some((app_id, app_secret)) = t.split_once(':') {
                    form.insert("appId".into(), Value::String(app_id.into()));
                    form.insert("appSecret".into(), Value::String(app_secret.into()));
                }
            }
        }
        "feishu" => {
            // 飞书: appId, appSecret, domain 直接保存
            if let Some(v) = saved.get("appId").and_then(|v| v.as_str()) {
                form.insert("appId".into(), Value::String(v.into()));
            }
            if let Some(v) = saved.get("appSecret").and_then(|v| v.as_str()) {
                form.insert("appSecret".into(), Value::String(v.into()));
            }
            if let Some(v) = saved.get("domain").and_then(|v| v.as_str()) {
                form.insert("domain".into(), Value::String(v.into()));
            }
        }
        _ => {
            // 通用：原样返回字符串类型字段
            if let Some(obj) = saved.as_object() {
                for (k, v) in obj {
                    if k == "enabled" {
                        continue;
                    }
                    if let Some(s) = v.as_str() {
                        form.insert(k.clone(), Value::String(s.into()));
                    }
                }
            }
        }
    }

    Ok(json!({ "exists": true, "values": Value::Object(form) }))
}

/// 保存平台配置到 openclaw.json
/// 前端传入的是表单字段，后端负责转换成 OpenClaw 要求的结构
#[tauri::command]
pub async fn save_messaging_platform(
    platform: String,
    form: Value,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let mut cfg = super::config::load_openclaw_json()?;

    let channels = cfg
        .as_object_mut()
        .ok_or("配置格式错误")?
        .entry("channels")
        .or_insert_with(|| json!({}));
    let channels_map = channels.as_object_mut().ok_or("channels 节点格式错误")?;

    let form_obj = form.as_object().ok_or("表单数据格式错误")?;

    match platform.as_str() {
        "discord" => {
            let mut entry = Map::new();

            // Bot Token
            if let Some(t) = form_obj.get("token").and_then(|v| v.as_str()) {
                entry.insert("token".into(), Value::String(t.trim().into()));
            }
            entry.insert("enabled".into(), Value::Bool(true));
            entry.insert("groupPolicy".into(), Value::String("allowlist".into()));
            entry.insert("dm".into(), json!({ "enabled": false }));
            entry.insert(
                "retry".into(),
                json!({
                    "attempts": 3,
                    "minDelayMs": 500,
                    "maxDelayMs": 30000,
                    "jitter": 0.1
                }),
            );

            // guildId + channelId 展开为 guilds 嵌套结构
            let guild_id = form_obj
                .get("guildId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !guild_id.is_empty() {
                let channel_id = form_obj
                    .get("channelId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let channel_key = if channel_id.is_empty() {
                    "*".to_string()
                } else {
                    channel_id
                };
                entry.insert(
                    "guilds".into(),
                    json!({
                        guild_id: {
                            "users": ["*"],
                            "requireMention": true,
                            "channels": {
                                channel_key: { "allow": true, "requireMention": true }
                            }
                        }
                    }),
                );
            }

            channels_map.insert("discord".into(), Value::Object(entry));
        }
        "telegram" => {
            let mut entry = Map::new();

            if let Some(t) = form_obj.get("botToken").and_then(|v| v.as_str()) {
                entry.insert("botToken".into(), Value::String(t.trim().into()));
            }
            entry.insert("enabled".into(), Value::Bool(true));

            // allowedUsers 逗号字符串 → allowFrom 数组
            if let Some(users_str) = form_obj.get("allowedUsers").and_then(|v| v.as_str()) {
                let users: Vec<Value> = users_str
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| Value::String(s.into()))
                    .collect();
                if !users.is_empty() {
                    entry.insert("allowFrom".into(), Value::Array(users));
                }
            }

            channels_map.insert("telegram".into(), Value::Object(entry));
        }
        "qqbot" => {
            let app_id = form_obj
                .get("appId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let app_secret = form_obj
                .get("appSecret")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if app_id.is_empty() || app_secret.is_empty() {
                return Err("AppID 和 AppSecret 不能为空".into());
            }

            let token = format!("{}:{}", app_id, app_secret);
            let mut entry = Map::new();
            entry.insert("token".into(), Value::String(token));
            entry.insert("enabled".into(), Value::Bool(true));

            channels_map.insert("qqbot".into(), Value::Object(entry));
        }
        "feishu" => {
            let app_id = form_obj
                .get("appId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let app_secret = form_obj
                .get("appSecret")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if app_id.is_empty() || app_secret.is_empty() {
                return Err("App ID 和 App Secret 不能为空".into());
            }

            let mut entry = Map::new();
            entry.insert("appId".into(), Value::String(app_id));
            entry.insert("appSecret".into(), Value::String(app_secret));
            entry.insert("enabled".into(), Value::Bool(true));
            entry.insert("connectionMode".into(), Value::String("websocket".into()));

            // 域名（默认 feishu，国际版选 lark）
            let domain = form_obj
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if !domain.is_empty() {
                entry.insert("domain".into(), Value::String(domain));
            }

            channels_map.insert("feishu".into(), Value::Object(entry));
        }
        _ => {
            // 通用平台：直接保存表单字段
            let mut entry = Map::new();
            for (k, v) in form_obj {
                entry.insert(k.clone(), v.clone());
            }
            entry.insert("enabled".into(), Value::Bool(true));
            channels_map.insert(platform.clone(), Value::Object(entry));
        }
    }

    // 写回配置并重载 Gateway
    super::config::save_openclaw_json(&cfg)?;

    // 触发 Gateway 重载使配置生效
    let _ = super::config::do_reload_gateway(&app).await;

    Ok(json!({ "ok": true }))
}

/// 删除指定平台配置
#[tauri::command]
pub async fn remove_messaging_platform(
    platform: String,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let mut cfg = super::config::load_openclaw_json()?;

    if let Some(channels) = cfg.get_mut("channels").and_then(|c| c.as_object_mut()) {
        channels.remove(&platform);
    }

    super::config::save_openclaw_json(&cfg)?;
    let _ = super::config::do_reload_gateway(&app).await;

    Ok(json!({ "ok": true }))
}

/// 切换平台启用/禁用
#[tauri::command]
pub async fn toggle_messaging_platform(
    platform: String,
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<Value, String> {
    let mut cfg = super::config::load_openclaw_json()?;

    if let Some(entry) = cfg
        .get_mut("channels")
        .and_then(|c| c.get_mut(&platform))
        .and_then(|v| v.as_object_mut())
    {
        entry.insert("enabled".into(), Value::Bool(enabled));
    } else {
        return Err(format!("平台 {} 未配置", platform));
    }

    super::config::save_openclaw_json(&cfg)?;
    let _ = super::config::do_reload_gateway(&app).await;

    Ok(json!({ "ok": true }))
}

/// 在线校验 Bot 凭证（调用平台 API 验证 Token 是否有效）
#[tauri::command]
pub async fn verify_bot_token(platform: String, form: Value) -> Result<Value, String> {
    let form_obj = form.as_object().ok_or("表单数据格式错误")?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {}", e))?;

    match platform.as_str() {
        "discord" => verify_discord(&client, form_obj).await,
        "telegram" => verify_telegram(&client, form_obj).await,
        "qqbot" => verify_qqbot(&client, form_obj).await,
        "feishu" => verify_feishu(&client, form_obj).await,
        _ => Ok(json!({
            "valid": true,
            "warnings": ["该平台暂不支持在线校验"]
        })),
    }
}

/// 列出当前已配置的平台清单
#[tauri::command]
pub async fn list_configured_platforms() -> Result<Value, String> {
    let cfg = super::config::load_openclaw_json()?;
    let mut result: Vec<Value> = vec![];

    if let Some(channels) = cfg.get("channels").and_then(|c| c.as_object()) {
        for (name, val) in channels {
            let enabled = val.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            result.push(json!({
                "id": name,
                "enabled": enabled
            }));
        }
    }

    Ok(json!(result))
}

// ── Discord 凭证校验 ──────────────────────────────────────

async fn verify_discord(
    client: &reqwest::Client,
    form: &Map<String, Value>,
) -> Result<Value, String> {
    let token = form
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if token.is_empty() {
        return Ok(json!({ "valid": false, "errors": ["Bot Token 不能为空"] }));
    }

    // 验证 Bot Token
    let me_resp = client
        .get("https://discord.com/api/v10/users/@me")
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| format!("Discord API 连接失败: {}", e))?;

    if me_resp.status() == 401 {
        return Ok(json!({ "valid": false, "errors": ["Bot Token 无效，请检查后重试"] }));
    }
    if !me_resp.status().is_success() {
        return Ok(json!({
            "valid": false,
            "errors": [format!("Discord API 返回异常: {}", me_resp.status())]
        }));
    }

    let me: Value = me_resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;
    if me.get("bot").and_then(|v| v.as_bool()) != Some(true) {
        return Ok(json!({
            "valid": false,
            "errors": ["提供的 Token 不属于 Bot 账号，请使用 Bot Token"]
        }));
    }

    let bot_name = me
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("未知");
    let mut details = vec![format!("Bot: @{}", bot_name)];

    // 验证 Guild（可选）
    let guild_id = form
        .get("guildId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if !guild_id.is_empty() {
        match client
            .get(format!("https://discord.com/api/v10/guilds/{}", guild_id))
            .header("Authorization", format!("Bot {}", token))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let guild: Value = resp.json().await.unwrap_or_default();
                let name = guild.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                details.push(format!("服务器: {}", name));
            }
            Ok(resp) if resp.status().as_u16() == 403 || resp.status().as_u16() == 404 => {
                return Ok(json!({
                    "valid": false,
                    "errors": [format!("无法访问服务器 {}，请确认 Bot 已加入该服务器", guild_id)]
                }));
            }
            _ => {
                details.push("服务器 ID 未能验证（网络问题）".into());
            }
        }
    }

    Ok(json!({
        "valid": true,
        "errors": [],
        "details": details
    }))
}

// ── QQ Bot 凭证校验 ──────────────────────────────────────

async fn verify_qqbot(
    client: &reqwest::Client,
    form: &Map<String, Value>,
) -> Result<Value, String> {
    let app_id = form
        .get("appId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let app_secret = form
        .get("appSecret")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();

    if app_id.is_empty() {
        return Ok(json!({ "valid": false, "errors": ["AppID 不能为空"] }));
    }
    if app_secret.is_empty() {
        return Ok(json!({ "valid": false, "errors": ["AppSecret 不能为空"] }));
    }

    // 通过 QQ Bot API 获取 access_token 验证凭证
    let resp = client
        .post("https://bots.qq.com/app/getAppAccessToken")
        .json(&json!({
            "appId": app_id,
            "clientSecret": app_secret
        }))
        .send()
        .await
        .map_err(|e| format!("QQ Bot API 连接失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    if body.get("access_token").and_then(|v| v.as_str()).is_some() {
        Ok(json!({
            "valid": true,
            "errors": [],
            "details": [format!("AppID: {}", app_id)]
        }))
    } else {
        let msg = body
            .get("message")
            .or_else(|| body.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("凭证无效，请检查 AppID 和 AppSecret");
        Ok(json!({
            "valid": false,
            "errors": [msg]
        }))
    }
}

// ── QQ Bot 插件安装（带日志流） ──────────────────────────

#[tauri::command]
pub async fn install_qqbot_plugin(app: tauri::AppHandle) -> Result<String, String> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use tauri::Emitter;

    let _ = app.emit("plugin-log", "正在安装 QQBot 社区插件 @sliverp/qqbot ...");
    let _ = app.emit("plugin-progress", 10);

    let mut child = crate::utils::openclaw_command()
        .args(["plugins", "install", "@sliverp/qqbot@latest"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 openclaw 失败: {}", e))?;

    let stderr = child.stderr.take();
    let app2 = app.clone();
    let handle = std::thread::spawn(move || {
        if let Some(pipe) = stderr {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                let _ = app2.emit("plugin-log", &line);
            }
        }
    });

    let _ = app.emit("plugin-progress", 30);

    let mut progress = 30;
    if let Some(pipe) = child.stdout.take() {
        for line in BufReader::new(pipe).lines().map_while(Result::ok) {
            let _ = app.emit("plugin-log", &line);
            if progress < 90 {
                progress += 10;
                let _ = app.emit("plugin-progress", progress);
            }
        }
    }

    let _ = handle.join();
    let _ = app.emit("plugin-progress", 95);

    let status = child
        .wait()
        .map_err(|e| format!("等待安装进程失败: {}", e))?;
    let _ = app.emit("plugin-progress", 100);

    if !status.success() {
        let _ = app.emit("plugin-log", "QQBot 插件安装失败");
        return Err("插件安装失败，请查看日志".into());
    }

    let _ = app.emit("plugin-log", "QQBot 插件安装完成");
    Ok("安装成功".into())
}

// ── Telegram 凭证校验 ─────────────────────────────────────

async fn verify_telegram(
    client: &reqwest::Client,
    form: &Map<String, Value>,
) -> Result<Value, String> {
    let bot_token = form
        .get("botToken")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if bot_token.is_empty() {
        return Ok(json!({ "valid": false, "errors": ["Bot Token 不能为空"] }));
    }

    let allowed = form
        .get("allowedUsers")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if allowed.is_empty() {
        return Ok(json!({ "valid": false, "errors": ["至少需要填写一个允许的用户 ID"] }));
    }

    let url = format!("https://api.telegram.org/bot{}/getMe", bot_token);
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Telegram API 连接失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    if body.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        let username = body
            .get("result")
            .and_then(|r| r.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("未知");
        Ok(json!({
            "valid": true,
            "errors": [],
            "details": [format!("Bot: @{}", username)]
        }))
    } else {
        let desc = body
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("Token 无效");
        Ok(json!({
            "valid": false,
            "errors": [desc]
        }))
    }
}

// ── 飞书凭证校验 ──────────────────────────────────────

async fn verify_feishu(
    client: &reqwest::Client,
    form: &Map<String, Value>,
) -> Result<Value, String> {
    let app_id = form
        .get("appId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let app_secret = form
        .get("appSecret")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();

    if app_id.is_empty() {
        return Ok(json!({ "valid": false, "errors": ["App ID 不能为空"] }));
    }
    if app_secret.is_empty() {
        return Ok(json!({ "valid": false, "errors": ["App Secret 不能为空"] }));
    }

    // 通过飞书 API 获取 tenant_access_token 验证凭证
    let domain = form
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let base_url = if domain == "lark" {
        "https://open.larksuite.com"
    } else {
        "https://open.feishu.cn"
    };

    let resp = client
        .post(format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            base_url
        ))
        .json(&json!({
            "app_id": app_id,
            "app_secret": app_secret
        }))
        .send()
        .await
        .map_err(|e| format!("飞书 API 连接失败: {}", e))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析响应失败: {}", e))?;

    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code == 0 {
        Ok(json!({
            "valid": true,
            "errors": [],
            "details": [format!("App ID: {}", app_id)]
        }))
    } else {
        let msg = body
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("凭证无效，请检查 App ID 和 App Secret");
        Ok(json!({
            "valid": false,
            "errors": [msg]
        }))
    }
}
