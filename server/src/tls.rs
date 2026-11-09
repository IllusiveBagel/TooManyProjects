//! HTTPS straight from the container, so apps that insist on it (such as
//! Claude Code signing in) work on a home network with no proxy or tunnel.
//!
//! By default the app makes its own small certificate authority the first
//! time it starts, keeps it in `data/tls`, and uses it to sign the
//! certificate it serves. People trust that authority once (Settings → HTTPS
//! has the download); after that the server certificate can be renewed or
//! cover new addresses without them doing anything again. Someone with a
//! real certificate sets `TMP_TLS_CERT` and `TMP_TLS_KEY` instead.

use std::{
    collections::BTreeSet,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use axum::{
    Json,
    extract::{Path as UrlPath, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use axum_server::tls_rustls::RustlsConfig;
use base64::{Engine, engine::general_purpose::STANDARD};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer,
    KeyPair, KeyUsagePurpose,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    config::Config,
    error::{AppError, AppResult},
};

const CA_YEARS: i64 = 10;
/// Under Apple's 825-day limit for certificates from an authority a person added.
const SERVER_DAYS: i64 = 365;
/// Renewed at start-up when it has less than this left.
const RENEW_DAYS: i64 = 30;
const MAX_EXTRA_HOSTS: usize = 50;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(https_info))
        .routes(routes!(add_host))
        .routes(routes!(remove_host))
        .routes(routes!(certificate))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct HttpsHost {
    /// A name or IP address the certificate covers.
    pub host: String,
    /// Added in Settings, so it can be removed there. The rest come from
    /// the server's own addresses, `TMP_PUBLIC_URL` and `TMP_TLS_HOSTS`.
    pub added: bool,
}

#[derive(Serialize, ToSchema)]
pub struct HttpsInfo {
    /// `off`, `generated` (the app's own certificate) or `own` (`TMP_TLS_CERT`).
    pub mode: String,
    /// The port HTTPS listens on inside the container.
    pub port: Option<u16>,
    /// Addresses the generated certificate covers.
    pub hosts: Vec<HttpsHost>,
    /// When the generated server certificate runs out. It renews itself.
    pub expires_at: Option<String>,
    /// SHA-256 of the certificate authority to trust, to compare after downloading.
    pub fingerprint: Option<String>,
}

/// What `data/tls/state.json` keeps between starts.
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    /// Added in Settings.
    extra_hosts: Vec<String>,
    /// What the current server certificate covers, and until when.
    issued_hosts: Vec<String>,
    issued_until: Option<String>,
}

enum Source {
    Own,
    Generated { dir: PathBuf, auto_hosts: Vec<String>, saved: Mutex<Saved>, ca_pem: String },
}

pub struct Tls {
    pub port: u16,
    pub rustls: RustlsConfig,
    source: Source,
}

impl Tls {
    /// Sets HTTPS up from the config, or `None` when `TMP_HTTPS` is off.
    pub async fn start(config: &Config) -> anyhow::Result<Option<Arc<Self>>> {
        let Some(bind) = config.https_bind else { return Ok(None) };
        if let (Some(cert), Some(key)) = (&config.tls_cert, &config.tls_key) {
            let rustls = RustlsConfig::from_pem_file(cert, key).await.with_context(|| {
                format!("reading TMP_TLS_CERT {} and TMP_TLS_KEY {}", cert.display(), key.display())
            })?;
            return Ok(Some(Arc::new(Self { port: bind.port(), rustls, source: Source::Own })));
        }
        let dir = config.data_dir.join("tls");
        std::fs::create_dir_all(&dir).context("creating data/tls")?;
        let (ca_pem, _) = load_or_make_ca(&dir)?;
        let saved: Saved = std::fs::read(dir.join("state.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let source =
            Source::Generated { dir, auto_hosts: auto_hosts(config), saved: Mutex::new(saved), ca_pem };
        let (cert, key) = source.ensure_cert()?;
        let rustls = RustlsConfig::from_pem(cert, key).await.context("loading the HTTPS certificate")?;
        Ok(Some(Arc::new(Self { port: bind.port(), rustls, source })))
    }

    async fn refresh(&self) -> anyhow::Result<()> {
        let (cert, key) = self.source.ensure_cert()?;
        self.rustls.reload_from_pem(cert, key).await.context("loading the HTTPS certificate")
    }
}

impl Source {
    /// The server certificate (with the authority after it) and its key,
    /// issued anew when missing, nearly out, or not covering the hosts it should.
    fn ensure_cert(&self) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        let Source::Generated { dir, auto_hosts, saved, .. } = self else {
            anyhow::bail!("not a generated certificate");
        };
        let mut saved = saved.lock().unwrap();
        let wanted = all_hosts(auto_hosts, &saved.extra_hosts);
        let fresh = saved
            .issued_until
            .as_deref()
            .and_then(|u| OffsetDateTime::parse(u, &Rfc3339).ok())
            .is_some_and(|until| until - OffsetDateTime::now_utc() > Duration::days(RENEW_DAYS));
        let have_files = dir.join("server.pem").is_file() && dir.join("server-key.pem").is_file();
        if !fresh || !have_files || saved.issued_hosts != wanted {
            let (ca_pem, ca_key) = load_or_make_ca(dir)?;
            let (cert, key, until) = issue(&ca_pem, &ca_key, &wanted)?;
            write_private(&dir.join("server-key.pem"), &key)?;
            std::fs::write(dir.join("server.pem"), format!("{cert}{ca_pem}"))?;
            saved.issued_hosts = wanted;
            saved.issued_until = Some(until);
            std::fs::write(dir.join("state.json"), serde_json::to_vec_pretty(&*saved)?)?;
            tracing::info!(hosts = ?saved.issued_hosts, "issued the HTTPS certificate");
        }
        Ok((std::fs::read(dir.join("server.pem"))?, std::fs::read(dir.join("server-key.pem"))?))
    }
}

impl Tls {
    fn info(&self) -> HttpsInfo {
        match &self.source {
            Source::Own => HttpsInfo {
                mode: "own".into(),
                port: Some(self.port),
                hosts: vec![],
                expires_at: None,
                fingerprint: None,
            },
            Source::Generated { auto_hosts, saved, ca_pem, .. } => {
                let saved = saved.lock().unwrap();
                let mut hosts: Vec<HttpsHost> =
                    auto_hosts.iter().map(|h| HttpsHost { host: h.clone(), added: false }).collect();
                hosts.extend(
                    saved
                        .extra_hosts
                        .iter()
                        .filter(|h| !auto_hosts.contains(h))
                        .map(|h| HttpsHost { host: h.clone(), added: true }),
                );
                HttpsInfo {
                    mode: "generated".into(),
                    port: Some(self.port),
                    hosts,
                    expires_at: saved.issued_until.clone(),
                    fingerprint: pem_der(ca_pem).map(|der| fingerprint(&der)),
                }
            }
        }
    }
}

fn off() -> HttpsInfo {
    HttpsInfo { mode: "off".into(), port: None, hosts: vec![], expires_at: None, fingerprint: None }
}

// ---------------------------------------------------------------------------
// Certificates

fn write_private(path: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// The authority's certificate and key, made on first use.
fn load_or_make_ca(dir: &Path) -> anyhow::Result<(String, String)> {
    let (cert_path, key_path) = (dir.join("ca.pem"), dir.join("ca-key.pem"));
    if let (Ok(cert), Ok(key)) = (std::fs::read_to_string(&cert_path), std::fs::read_to_string(&key_path)) {
        return Ok((cert, key));
    }
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    // A random tag, so two installs' authorities are told apart in a keychain.
    let tag: String = crate::time::new_id().chars().rev().take(6).collect();
    name.push(DnType::CommonName, format!("TooManyProjects local authority {tag}"));
    name.push(DnType::OrganizationName, "TooManyProjects");
    params.distinguished_name = name;
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    params.key_usages =
        vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign, KeyUsagePurpose::DigitalSignature];
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(365 * CA_YEARS);
    let cert = params.self_signed(&key)?;
    write_private(&key_path, &key.serialize_pem())?;
    std::fs::write(&cert_path, cert.pem())?;
    tracing::info!("made a certificate authority for HTTPS in {}", dir.display());
    Ok((cert.pem(), key.serialize_pem()))
}

/// A server certificate for `hosts`, signed by the authority. Returns the
/// certificate, its key and when it runs out.
fn issue(ca_pem: &str, ca_key: &str, hosts: &[String]) -> anyhow::Result<(String, String, String)> {
    let issuer = Issuer::from_ca_cert_pem(ca_pem, KeyPair::from_pem(ca_key)?)?;
    let key = KeyPair::generate()?;
    let mut params = CertificateParams::new(hosts.to_vec())?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "TooManyProjects");
    params.distinguished_name = name;
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature, KeyUsagePurpose::KeyEncipherment];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(SERVER_DAYS);
    let cert = params.signed_by(&key, &issuer)?;
    Ok((cert.pem(), key.serialize_pem(), params.not_after.format(&Rfc3339)?))
}

fn pem_der(pem: &str) -> Option<Vec<u8>> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    STANDARD.decode(body).ok()
}

fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

// ---------------------------------------------------------------------------
// Hosts

/// A name or IP a certificate can cover, normalised, or `None`.
pub fn clean_host(host: &str) -> Option<String> {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let h = h.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(&h).to_string();
    if let Ok(ip) = h.parse::<IpAddr>() {
        return Some(ip.to_string());
    }
    let ok = !h.is_empty()
        && h.len() <= 253
        && h.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        });
    ok.then_some(h)
}

/// The host part of `TMP_PUBLIC_URL`, without a port.
fn public_host(config: &Config) -> Option<String> {
    let url = reqwest::Url::parse(config.public_url.as_deref()?).ok()?;
    clean_host(url.host_str()?)
}

/// What the certificate always covers: this computer, the server's own
/// network addresses, `TMP_PUBLIC_URL` and `TMP_TLS_HOSTS`.
fn auto_hosts(config: &Config) -> Vec<String> {
    let mut hosts = vec!["localhost".to_string(), "127.0.0.1".into(), "::1".into()];
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        hosts.extend(
            ifaces.iter().filter(|i| !i.is_loopback() && i.ip().is_ipv4()).map(|i| i.ip().to_string()),
        );
    }
    hosts.extend(public_host(config));
    hosts.extend(config.tls_hosts.iter().filter_map(|h| clean_host(h)));
    let mut seen = BTreeSet::new();
    hosts.retain(|h| seen.insert(h.clone()));
    hosts
}

fn all_hosts(auto: &[String], extra: &[String]) -> Vec<String> {
    let mut all = auto.to_vec();
    all.extend(extra.iter().filter(|h| !auto.contains(h)).cloned());
    all
}

// ---------------------------------------------------------------------------
// Handlers

fn require_owner(user: &crate::auth::User) -> AppResult<()> {
    if user.is_owner() { Ok(()) } else { Err(AppError::Forbidden("Only the owner can change HTTPS.".into())) }
}

fn generated(state: &AppState) -> AppResult<&Arc<Tls>> {
    match &state.tls {
        Some(tls) if matches!(tls.source, Source::Generated { .. }) => Ok(tls),
        _ => Err(AppError::BadRequest(
            "not_generated",
            "HTTPS isn't using the app's own certificate, so there are no addresses to change.".into(),
        )),
    }
}

/// Whether HTTPS is on, and what its certificate covers.
#[utoipa::path(get, path = "/https", tag = "system", responses((status = 200, body = HttpsInfo)))]
async fn https_info(State(state): State<AppState>, _user: CurrentUser) -> Json<HttpsInfo> {
    Json(state.tls.as_ref().map_or_else(off, |t| t.info()))
}

#[derive(Deserialize, ToSchema)]
pub struct AddHostRequest {
    /// A name such as `nas.local`, or an IP address such as `192.168.1.50`.
    pub host: String,
}

/// Cover another address with the certificate. The owner only. Anyone who
/// already trusts the app's authority needn't do anything again.
#[utoipa::path(post, path = "/https/hosts", tag = "system", request_body = AddHostRequest,
    responses((status = 200, body = HttpsInfo), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody)))]
async fn add_host(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<AddHostRequest>,
) -> AppResult<Json<HttpsInfo>> {
    require_owner(&user)?;
    let tls = generated(&state)?;
    let host = clean_host(&req.host).ok_or_else(|| {
        AppError::BadRequest(
            "invalid_host",
            "Use a name like nas.local or an address like 192.168.1.50.".into(),
        )
    })?;
    if let Source::Generated { saved, auto_hosts, .. } = &tls.source {
        let mut saved = saved.lock().unwrap();
        if !auto_hosts.contains(&host) && !saved.extra_hosts.contains(&host) {
            if saved.extra_hosts.len() >= MAX_EXTRA_HOSTS {
                return Err(AppError::BadRequest(
                    "too_many_hosts",
                    "Remove an address first; 50 is the most.".into(),
                ));
            }
            saved.extra_hosts.push(host);
        }
    }
    tls.refresh().await?;
    Ok(Json(tls.info()))
}

/// Stop covering an address added in Settings. The owner only.
#[utoipa::path(delete, path = "/https/hosts/{host}", tag = "system", params(("host" = String, Path)),
    responses((status = 200, body = HttpsInfo), (status = 404, body = crate::error::ErrorBody)))]
async fn remove_host(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    UrlPath(host): UrlPath<String>,
) -> AppResult<Json<HttpsInfo>> {
    require_owner(&user)?;
    let tls = generated(&state)?;
    let host = clean_host(&host).ok_or(AppError::NotFound)?;
    if let Source::Generated { saved, .. } = &tls.source {
        let mut saved = saved.lock().unwrap();
        let before = saved.extra_hosts.len();
        saved.extra_hosts.retain(|h| *h != host);
        if saved.extra_hosts.len() == before {
            return Err(AppError::NotFound);
        }
    }
    tls.refresh().await?;
    Ok(Json(tls.info()))
}

/// The certificate authority to trust, as a PEM file. Public, like any
/// certificate, so a phone can fetch it before anyone signs in.
#[utoipa::path(get, path = "/https/certificate", tag = "system",
    responses((status = 200, content_type = "application/x-pem-file", body = String),
        (status = 404, body = crate::error::ErrorBody)))]
async fn certificate(State(state): State<AppState>, _headers: HeaderMap) -> AppResult<Response> {
    let Some(Source::Generated { ca_pem, .. }) = state.tls.as_ref().map(|t| &t.source) else {
        return Err(AppError::NotFound);
    };
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-pem-file".to_string()),
            (header::CONTENT_DISPOSITION, crate::notes::transfer::attachment("TooManyProjects-CA.pem")),
        ],
        ca_pem.clone(),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_cleaned() {
        assert_eq!(clean_host(" NAS.local. ").as_deref(), Some("nas.local"));
        assert_eq!(clean_host("192.168.1.50").as_deref(), Some("192.168.1.50"));
        assert_eq!(clean_host("[::1]").as_deref(), Some("::1"));
        assert_eq!(clean_host("bad host"), None);
        assert_eq!(clean_host("a..b"), None);
        assert_eq!(clean_host("-x.com"), None);
    }

    #[test]
    fn issues_a_certificate_signed_by_its_authority() {
        let dir = tempfile::tempdir().unwrap();
        let (ca, key) = load_or_make_ca(dir.path()).unwrap();
        // The second call reads the same authority back.
        assert_eq!(load_or_make_ca(dir.path()).unwrap().0, ca);
        let (cert, _, until) = issue(&ca, &key, &["localhost".into(), "192.168.1.50".into()]).unwrap();
        assert!(cert.starts_with("-----BEGIN CERTIFICATE-----"));
        assert!(OffsetDateTime::parse(&until, &Rfc3339).is_ok());
        assert_eq!(fingerprint(&pem_der(&ca).unwrap()).len(), 32 * 3 - 1);
    }
}
