//! Blocking HTTP client with cookie jar, gzip, rustls, and connection reuse.

use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, REFERER};
use thiserror::Error;

use crate::config::Config;
use crate::model::Action;
use crate::parser::is_login_page;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
                              (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("gagal membuat HTTP client: {0}")]
    Build(#[source] reqwest::Error),
    #[error("request HTTP gagal: {0}")]
    Http(#[source] reqwest::Error),
    #[error("nilai header tidak valid: {0}")]
    InvalidHeader(#[source] reqwest::header::InvalidHeaderValue),
}

#[derive(Debug, Error)]
pub enum DumpError {
    #[error("Login GAGAL -- cek NIM/password.")]
    LoginFailed,
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("gagal menulis file dump {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginOutcome {
    Authenticated { html: String },
    Failed { redirect_url: String },
}

/// Reusable HTTP client for Polban academic portal.
#[derive(Clone)]
pub struct AbsenClient {
    http: Client,
}

impl AbsenClient {
    /// Construct a single pooled `reqwest::blocking::Client` with cookie store,
    /// gzip decompression, rustls, TCP nodelay, and Chrome 126 User-Agent.
    pub fn new(base_url: &str) -> Result<Self, ClientError> {
        let mut headers = HeaderMap::new();
        let referer = format!("{}/", base_url.trim_end_matches('/'));
        headers.insert(
            REFERER,
            HeaderValue::from_str(&referer).map_err(ClientError::InvalidHeader)?,
        );

        let http = Client::builder()
            .cookie_store(true)
            .gzip(true)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .tcp_nodelay(true)
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .build()
            .map_err(ClientError::Build)?;

        Ok(Self { http })
    }

    /// Perform `POST /laman/login` followed by `GET target_url` and return the
    /// fetched target HTML on success so the caller avoids a duplicate GET.
    pub fn login(&self, cfg: &Config) -> Result<LoginOutcome, ClientError> {
        let login_url = cfg.login_url();
        let form = [
            ("username", cfg.username.as_str()),
            ("password", cfg.password.as_str()),
            ("submit", ""),
        ];
        self.http
            .post(&login_url)
            .form(&form)
            .send()
            .map_err(ClientError::Http)?;

        let (final_url, html) = self.get_target(cfg)?;
        if is_login_page(&html) {
            Ok(LoginOutcome::Failed {
                redirect_url: final_url,
            })
        } else {
            Ok(LoginOutcome::Authenticated { html })
        }
    }

    /// Fetch `target_url` and return `(final_url, html)`.
    pub fn get_target(&self, cfg: &Config) -> Result<(String, String), ClientError> {
        let resp = self
            .http
            .get(&cfg.target_url)
            .send()
            .map_err(ClientError::Http)?;
        let final_url = resp.url().to_string();
        let html = resp.text().map_err(ClientError::Http)?;
        Ok((final_url, html))
    }

    /// Submit AJAX attendance POST with `X-Requested-With`, `Referer`, and `Origin`.
    pub fn post_absen(&self, cfg: &Config, action: &Action) -> Result<(u16, String), ClientError> {
        let resp = self
            .http
            .post(&action.url)
            .form(&action.data.as_form_pairs())
            .header("X-Requested-With", "XMLHttpRequest")
            .header(REFERER, &cfg.target_url)
            .header("Origin", &cfg.base_url)
            .send()
            .map_err(ClientError::Http)?;
        let status = resp.status().as_u16();
        let body = resp.text().map_err(ClientError::Http)?;
        Ok((status, body))
    }

    /// Login and save raw target HTML to `out_path` for diagnostics (`--dump`).
    pub fn dump_absen_html(&self, cfg: &Config, out_path: &Path) -> Result<PathBuf, DumpError> {
        let html = match self.login(cfg)? {
            LoginOutcome::Authenticated { html } => html,
            LoginOutcome::Failed { .. } => return Err(DumpError::LoginFailed),
        };
        std::fs::write(out_path, html).map_err(|source| DumpError::Io {
            path: out_path.to_path_buf(),
            source,
        })?;
        Ok(out_path.to_path_buf())
    }
}
