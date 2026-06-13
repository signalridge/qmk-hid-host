use std::{path::PathBuf, sync::OnceLock};

#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WeatherConfig {
    pub url: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub devices: Vec<Device>,
    pub layouts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_delay: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<WeatherConfig>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(serialize_with = "hex_to_string", deserialize_with = "string_to_hex")]
    pub product_id: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_page: Option<u16>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn get_config() -> &'static Config {
    CONFIG.get().unwrap()
}

/// Command-line overrides that take precedence over the config file. When any of
/// these is set the config file is not auto-created, so the app can run without a
/// `qmk-hid-host.json` at all (e.g. `qmk-hid-host --product-id 0x1988 --layout en`).
#[derive(Default)]
pub struct ConfigOverrides {
    pub product_id: Option<String>,
    pub name: Option<String>,
    pub usage: Option<String>,
    pub usage_page: Option<String>,
    pub layouts: Vec<String>,
    pub reconnect_delay: Option<u64>,
    pub weather_url: Option<String>,
}

impl ConfigOverrides {
    fn has_any(&self) -> bool {
        self.product_id.is_some()
            || self.name.is_some()
            || self.usage.is_some()
            || self.usage_page.is_some()
            || !self.layouts.is_empty()
            || self.reconnect_delay.is_some()
            || self.weather_url.is_some()
    }
}

fn parse_hex_u16(value: &str) -> u16 {
    let hex = value.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(hex, 16).unwrap_or_else(|e| {
        tracing::error!("Invalid hex value '{}': {}", value, e);
        std::process::exit(1);
    })
}

fn default_config() -> Config {
    Config {
        devices: vec![Device {
            name: None,
            product_id: 0x0844,
            usage: None,
            usage_page: None,
        }],
        layouts: vec!["en".to_string()],
        reconnect_delay: None,
        weather: Some(WeatherConfig {
            url: "wttr.in/Hamburg?format=%t".to_string(),
        }),
    }
}

pub fn load_config(path: PathBuf, overrides: ConfigOverrides) -> &'static Config {
    if let Some(config) = CONFIG.get() {
        return config;
    }

    // base config: the file if present, otherwise the built-in default. The
    // default template is only written to disk when there are no CLI overrides.
    let mut config = if let Ok(file) = std::fs::read_to_string(&path) {
        serde_json::from_str::<Config>(&file)
            .map_err(|e| tracing::error!("Incorrect config file: {}", e))
            .unwrap()
    } else if overrides.has_any() {
        default_config()
    } else {
        let default_config = default_config();
        let file_content = serde_json::to_string_pretty(&default_config).unwrap();
        std::fs::write(&path, &file_content)
            .map_err(|e| tracing::error!("Error while saving config file to {:?}: {}", path, e))
            .unwrap();
        tracing::info!("New config file created at {:?}", path);
        default_config
    };

    // apply CLI overrides on top of the base config
    if let Some(product_id) = overrides.product_id.as_deref() {
        config.devices = vec![Device {
            name: overrides.name.clone(),
            product_id: parse_hex_u16(product_id),
            usage: overrides.usage.as_deref().map(parse_hex_u16),
            usage_page: overrides.usage_page.as_deref().map(parse_hex_u16),
        }];
    }
    if !overrides.layouts.is_empty() {
        config.layouts = overrides.layouts.clone();
    }
    if let Some(reconnect_delay) = overrides.reconnect_delay {
        config.reconnect_delay = Some(reconnect_delay);
    }
    if let Some(url) = overrides.weather_url.clone() {
        config.weather = Some(WeatherConfig { url });
    }

    CONFIG.get_or_init(|| config)
}

fn string_to_hex<'de, D>(deserializer: D) -> Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: &str = serde::Deserialize::deserialize(deserializer)?;
    let hex = value.trim_start_matches("0x");
    return u16::from_str_radix(hex, 16).map_err(serde::de::Error::custom);
}

fn hex_to_string<S>(value: &u16, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&format!("0x{:04x}", value))
}
