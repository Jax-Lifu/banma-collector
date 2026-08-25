use super::*;

#[tauri::command]
pub(crate) async fn request_sms(
    state: State<'_, AppState>,
    request: SmsRequest,
) -> Result<(), String> {
    let phone = normalize_phone(&request.phone)?;
    let encrypted_phone = encrypt_login_value(&phone)?;
    let response = client_from(&state, &request.product)?
        .post(format!("{ACCOUNT_HOST}/verifier/android/sms"))
        .query(&common_params(&request.product)?)
        .form(&[("phone", encrypted_phone)])
        .send()
        .await
        .map_err(|e| format!("发送验证码失败：{e}"))?;
    response_json(response, "发送验证码").await?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn phone_login(
    state: State<'_, AppState>,
    request: LoginRequest,
) -> Result<LoginSession, String> {
    let phone = normalize_phone(&request.phone)?;
    let code = request.code.trim();
    if !(4..=8).contains(&code.len()) || !code.chars().all(|c| c.is_ascii_digit()) {
        return Err("请输入短信中的数字验证码".into());
    }
    let response = client_from(&state, &request.product)?
        .post(format!("{ACCOUNT_HOST}/accounts/android/login"))
        .query(&common_params(&request.product)?)
        .form(&[
            ("phone", encrypt_login_value(&phone)?),
            ("verification", encrypt_login_value(code)?),
            ("targetUserId", "0".into()),
            ("yfd_s", String::new()),
            ("yfd_o", String::new()),
        ])
        .send()
        .await
        .map_err(|e| format!("手机号登录失败：{e}"))?;
    let user = response_json(response, "手机号登录").await?;
    let product_name = request.product.clone();
    let session = LoginSession {
        logged_in: true,
        phone_masked: Some(mask_phone(&phone)),
        product: Some(request.product),
        user_id: json_find_string(&user, &["userId", "user_id", "id"]),
        nickname: json_find_string(&user, &["nickname", "displayName", "name"]),
    };
    state
        .sessions
        .lock()
        .map_err(|_| "登录会话状态异常")?
        .insert(product_name.clone(), session.clone());
    save_product_session_and_cookies(&state, &product_name);
    Ok(session)
}
#[tauri::command]
pub(crate) fn login_session(
    state: State<'_, AppState>,
    request: ProductRequest,
) -> Result<LoginSession, String> {
    product_meta(&request.product)?;
    state
        .sessions
        .lock()
        .map_err(|_| "登录会话状态异常")?
        .get(&request.product)
        .cloned()
        .ok_or_else(|| "产品登录会话不存在".into())
}

#[tauri::command]
pub(crate) async fn logout(
    state: State<'_, AppState>,
    request: ProductRequest,
) -> Result<LoginSession, String> {
    product_meta(&request.product)?;
    let _ = client_from(&state, &request.product)?
        .post(format!("{ACCOUNT_HOST}/accounts/android/logout"))
        .query(&common_params(&request.product)?)
        .send()
        .await;
    let jar = Arc::new(reqwest::cookie::Jar::default());
    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .user_agent("ZebraAndroid/1.0 BanmaCollector/0.1")
        .build()
        .map_err(|e| e.to_string())?;
    state
        .clients
        .lock()
        .map_err(|_| "登录会话状态异常")?
        .insert(request.product.clone(), client);
    state
        .jars
        .lock()
        .map_err(|_| "登录会话状态异常")?
        .insert(request.product.clone(), jar);
    let session = LoginSession::default();
    state
        .sessions
        .lock()
        .map_err(|_| "登录会话状态异常")?
        .insert(request.product.clone(), session.clone());
    save_product_session_and_cookies(&state, &request.product);
    Ok(session)
}
