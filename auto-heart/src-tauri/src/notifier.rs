/// Auto-Heart 通知发送模块
///
/// 产品方案 §3.3：日报生成后确认发送，支持一键发往钉钉、飞书
///
/// 两者均使用 Incoming Webhook（机器人），用户只需配置 URL，
/// 无需任何 OAuth / 登录，满足隐私设计原则（用户自主控制）。
use reqwest::Client;

// ──────────────────────────────────────────────
// 钉钉机器人
// Webhook: https://oapi.dingtalk.com/robot/send?access_token=xxx
// ──────────────────────────────────────────────

pub async fn send_to_dingtalk(webhook_url: &str, title: &str, content: &str) -> Result<(), String> {
    if webhook_url.is_empty() {
        return Err("钉钉 Webhook URL 未配置".to_string());
    }

    let client = Client::new();
    let body = serde_json::json!({
        "msgtype": "markdown",
        "markdown": {
            "title": title,
            "text": format!("## {}\n\n{}", title, content)
        }
    });

    let resp = client
        .post(webhook_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status();
    let resp_text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("钉钉返回 HTTP {}: {}", status, resp_text));
    }

    // 钉钉正常时返回 {"errcode":0,"errmsg":"ok"}
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp_text) {
        let errcode = json.get("errcode").and_then(|v| v.as_i64()).unwrap_or(0);
        if errcode != 0 {
            let errmsg = json
                .get("errmsg")
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            return Err(format!("钉钉错误({}): {}", errcode, errmsg));
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────
// 飞书机器人
// Webhook: https://open.feishu.cn/open-apis/bot/v2/hook/xxx
// ──────────────────────────────────────────────

pub async fn send_to_feishu(webhook_url: &str, content: &str) -> Result<(), String> {
    if webhook_url.is_empty() {
        return Err("飞书 Webhook URL 未配置".to_string());
    }

    let client = Client::new();
    let body = serde_json::json!({
        "msg_type": "text",
        "content": {
            "text": content
        }
    });

    let resp = client
        .post(webhook_url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Err(format!("飞书返回 HTTP {}: {}", status, err_text));
    }

    // 飞书正常时返回 {"code":0,"msg":"success"}
    let resp_text = resp.text().await.unwrap_or_default();
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&resp_text) {
        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        if code != 0 {
            let msg = json
                .get("msg")
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            return Err(format!("飞书错误({}): {}", code, msg));
        }
    }

    Ok(())
}
