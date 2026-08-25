use super::*;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LoginSession {
    pub(crate) logged_in: bool,
    pub(crate) phone_masked: Option<String>,
    pub(crate) product: Option<String>,
    pub(crate) user_id: Option<String>,
    pub(crate) nickname: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedProductData {
    session: LoginSession,
    #[serde(default, rename = "cookies", skip_serializing)]
    legacy_cookies: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PersistedStorage {
    products: HashMap<String, PersistedProductData>,
}

fn get_storage_path() -> PathBuf {
    if let Some(mut dir) = std::env::var_os("APPDATA").map(PathBuf::from) {
        dir.push("BanmaCollector");
        let _ = std::fs::create_dir_all(&dir);
        dir.push("sessions.json");
        return dir;
    }
    if let Some(home) = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
    {
        let mut dir = home;
        dir.push(".banma_collector");
        let _ = std::fs::create_dir_all(&dir);
        dir.push("sessions.json");
        return dir;
    }
    PathBuf::from("banma_sessions.json")
}

fn load_persisted_storage() -> PersistedStorage {
    let path = get_storage_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(storage) = serde_json::from_str::<PersistedStorage>(&content) {
            return storage;
        }
    }
    PersistedStorage::default()
}

fn save_persisted_storage(storage: &PersistedStorage) {
    let path = get_storage_path();
    if let Ok(content) = serde_json::to_string_pretty(storage) {
        let _ = std::fs::write(&path, content);
    }
}

pub(crate) fn save_product_session_and_cookies(state: &AppState, product: &str) {
    let session = state
        .sessions
        .lock()
        .ok()
        .and_then(|m| m.get(product).cloned())
        .unwrap_or_default();
    let conan_url = url::Url::parse(ACCOUNT_HOST).expect("valid conan url");
    let cookies = state
        .jars
        .lock()
        .ok()
        .and_then(|m| m.get(product).cloned())
        .and_then(|jar| jar.cookies(&conan_url))
        .and_then(|val| val.to_str().ok().map(str::to_owned));

    let mut storage = load_persisted_storage();
    if let Some(cookies) = cookies.filter(|value| !value.trim().is_empty()) {
        if let Err(error) = secure_storage::save_cookie(product, &cookies) {
            debug_log!(
                "secure session save failed product={} error={}",
                product,
                error
            );
        }
    } else {
        if let Err(error) = secure_storage::delete_cookie(product) {
            debug_log!(
                "secure session delete failed product={} error={}",
                product,
                error
            );
        }
    }
    storage.products.insert(
        product.to_string(),
        PersistedProductData {
            session,
            legacy_cookies: None,
        },
    );
    save_persisted_storage(&storage);
}

pub(crate) struct AppState {
    pub(crate) clients: Mutex<HashMap<String, reqwest::Client>>,
    pub(crate) jars: Mutex<HashMap<String, Arc<reqwest::cookie::Jar>>>,
    pub(crate) sessions: Mutex<HashMap<String, LoginSession>>,
    pub(crate) download_cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub(crate) download_generation: Arc<AtomicU64>,
    pub(crate) album_load_generation: AtomicU64,
}

impl AppState {
    pub(crate) fn new() -> Self {
        let storage = load_persisted_storage();
        let conan_url = url::Url::parse(ACCOUNT_HOST).expect("valid conan url");
        let extra_url = url::Url::parse("https://yuanfudao.com").expect("valid url");

        let mut clients = HashMap::new();
        let mut jars = HashMap::new();
        let mut sessions = HashMap::new();

        for product in ["pedia", "aioral", "zebra"] {
            let jar = Arc::new(reqwest::cookie::Jar::default());
            let mut session = LoginSession::default();

            if let Some(persisted) = storage.products.get(product) {
                session = persisted.session.clone();
                let protected_cookie = secure_storage::load_cookie(product).ok().flatten();
                let cookies = protected_cookie
                    .clone()
                    .or_else(|| persisted.legacy_cookies.clone());
                if let Some(ref cookies_str) = cookies {
                    for cookie in cookies_str.split(';') {
                        let c = cookie.trim();
                        if !c.is_empty() {
                            jar.add_cookie_str(c, &conan_url);
                            jar.add_cookie_str(c, &extra_url);
                        }
                    }
                }
                if protected_cookie.is_none() {
                    if let Some(ref legacy) = persisted.legacy_cookies {
                        if let Err(error) = secure_storage::save_cookie(product, legacy) {
                            debug_log!(
                                "legacy session migration failed product={} error={}",
                                product,
                                error
                            );
                        }
                    }
                }
            }

            let client = reqwest::Client::builder()
                .cookie_provider(jar.clone())
                .user_agent("ZebraAndroid/1.0 BanmaCollector/0.1")
                .build()
                .expect("create HTTP client");

            clients.insert(product.to_string(), client);
            jars.insert(product.to_string(), jar);
            sessions.insert(product.to_string(), session);
        }

        Self {
            clients: Mutex::new(clients),
            jars: Mutex::new(jars),
            sessions: Mutex::new(sessions),
            download_cancellations: Mutex::new(HashMap::new()),
            download_generation: Arc::new(AtomicU64::new(0)),
            album_load_generation: AtomicU64::new(0),
        }
    }
}

pub(crate) fn client_from(
    state: &State<'_, AppState>,
    product: &str,
) -> Result<reqwest::Client, String> {
    product_meta(product)?;
    state
        .clients
        .lock()
        .map_err(|_| "登录会话状态异常")?
        .get(product)
        .cloned()
        .ok_or_else(|| "产品网络会话不存在".into())
}
