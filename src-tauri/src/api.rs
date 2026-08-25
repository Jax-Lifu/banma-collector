use super::*;

pub(crate) fn product_meta(product: &str) -> Result<(i64, &'static str), String> {
    match product {
        "pedia" => Ok((5_000_013, "2.37.0")),
        "aioral" => Ok((5_000_035, "1.16.0")),
        "zebra" => Ok((513, "7.51.0")),
        _ => Err("不支持的斑马产品".into()),
    }
}

pub(crate) fn common_params(product: &str) -> Result<Vec<(&'static str, String)>, String> {
    let (product_id, version) = product_meta(product)?;
    Ok(vec![
        ("_productId", product_id.to_string()),
        ("version", version.into()),
        ("av", "11".into()),
        ("platform", "android35".into()),
        ("device-type", "PC".into()),
    ])
}

pub(crate) fn encrypt_login_value(value: &str) -> Result<String, String> {
    let der = BASE64
        .decode(LOGIN_PUBLIC_KEY)
        .map_err(|_| "登录公钥无效")?;
    let key = RsaPublicKey::from_public_key_der(&der).map_err(|_| "登录公钥解析失败")?;
    let encrypted = key
        .encrypt(&mut thread_rng(), Pkcs1v15Encrypt, value.as_bytes())
        .map_err(|_| "登录字段加密失败")?;
    Ok(BASE64.encode(encrypted))
}

pub(crate) fn normalize_phone(phone: &str) -> Result<String, String> {
    let clean = phone
        .trim()
        .replace([' ', '-'], "")
        .trim_start_matches("+86")
        .to_string();
    let valid = Regex::new(r"^1[3-9]\d{9}$").expect("valid phone regex");
    if valid.is_match(&clean) {
        Ok(clean)
    } else {
        Err("请输入有效的中国大陆手机号".into())
    }
}

pub(crate) fn mask_phone(phone: &str) -> String {
    format!("{}****{}", &phone[..3], &phone[7..])
}

pub(crate) async fn response_json(
    response: reqwest::Response,
    action: &str,
) -> Result<serde_json::Value, String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("{action}响应读取失败：{e}"))?;
    if !status.is_success() {
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("message")
                    .or_else(|| v.get("error"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| text.chars().take(160).collect());
        return Err(format!(
            "{action}失败（HTTP {}）：{}",
            status.as_u16(),
            if message.is_empty() {
                "服务器未返回原因"
            } else {
                &message
            }
        ));
    }
    if text.trim().is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| format!("{action}响应格式异常：{e}"))
}

pub(crate) fn product_session(
    state: &State<'_, AppState>,
    product: &str,
) -> Result<LoginSession, String> {
    let session = state
        .sessions
        .lock()
        .map_err(|_| "登录会话状态异常")?
        .get(product)
        .cloned()
        .unwrap_or_default();
    if !session.logged_in {
        return Err("LOGIN_REQUIRED:请先登录当前产品".into());
    }
    if session.user_id.is_none() {
        return Err("当前账号缺少用户标识，请退出后重新登录".into());
    }
    Ok(session)
}

pub(crate) async fn fetch_product_json(
    state: &State<'_, AppState>,
    product: &str,
    url: String,
    query: Vec<(&str, String)>,
    action: &str,
) -> Result<serde_json::Value, String> {
    let mut params = common_params(product)?;
    params.extend(query);
    let response = client_from(state, product)?
        .get(url)
        .query(&params)
        .send()
        .await
        .map_err(|e| format!("{action}失败：{e}"))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err("LOGIN_REQUIRED:登录已失效，请重新登录".into());
    }
    response_json(response, action).await
}
