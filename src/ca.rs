use color_eyre::eyre::{Context, ContextCompat};
use hudsucker::{
    certificate_authority::RcgenAuthority,
    rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose},
    rustls::crypto::aws_lc_rs,
};
use std::path::{Path, PathBuf};
use tokio::fs;

const CA_CERT_FILENAME: &str = "ca.cer";
const CA_KEY_FILENAME: &str = "ca.key";

/// Number of TLS certificates to cache in memory (per-hostname).
const CERT_CACHE_SIZE: u64 = 1_000;

pub struct CaFiles {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl CaFiles {
    pub fn new(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            cert_path: dir.join(CA_CERT_FILENAME),
            key_path: dir.join(CA_KEY_FILENAME),
        }
    }

    pub fn both_exist(&self) -> bool {
        self.cert_path.exists() && self.key_path.exists()
    }
}

/// Loads a persistent CA from disk, or generates and saves a new one.
///
/// # Directory layout
/// ```
/// ca_dir/
/// ├── ca.cer   ← PEM-encoded CA certificate (install this in clients)
/// └── ca.key   ← PEM-encoded private key    (keep this secret)
/// ```
pub async fn load_or_create(ca_dir: impl AsRef<Path>) -> crate::Result<RcgenAuthority> {
    let ca_dir = ca_dir.as_ref();
    fs::create_dir_all(ca_dir)
        .await
        .wrap_err("Failed to create CA directory")?;

    let files = CaFiles::new(ca_dir);

    let (cert_pem, key_pem) = if files.both_exist() {
        load_from_disk(&files).await?
    } else {
        generate_and_save(&files).await?
    };

    build_authority(&cert_pem, &key_pem)
}

// ── Private helpers ──────────────────────────────────────────────────────────

async fn load_from_disk(files: &CaFiles) -> crate::Result<(String, String)> {
    let cert_pem = fs::read_to_string(&files.cert_path)
        .await
        .wrap_err("Failed to read CA certificate")?;
    let key_pem = fs::read_to_string(&files.key_path)
        .await
        .wrap_err("Failed to read CA private key")?;

    tracing::info!(
        cert = %files.cert_path.display(),
        key  = %files.key_path.display(),
        "Loaded existing CA from disk",
    );

    Ok((cert_pem, key_pem))
}

async fn generate_and_save(files: &CaFiles) -> crate::Result<(String, String)> {
    tracing::info!("No CA found — generating a new one");

    let (cert_pem, key_pem) = generate_ca_pem()?;

    fs::write(&files.cert_path, &cert_pem)
        .await
        .wrap_err("Failed to write CA certificate")?;
    fs::write(&files.key_path, &key_pem)
        .await
        .wrap_err("Failed to write CA private key")?;

    tracing::info!(
        cert = %files.cert_path.display(),
        key  = %files.key_path.display(),
        "New CA saved to disk — install `{}` in your clients",
        files.cert_path.display(),
    );

    Ok((cert_pem, key_pem))
}

/// Generates a CA key pair and self-signed certificate, returns (cert_pem, key_pem).
fn generate_ca_pem() -> crate::Result<(String, String)> {
    let key_pair = KeyPair::generate().wrap_err("Failed to generate CA key pair")?;

    let mut params = CertificateParams::default();
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    params
        .distinguished_name
        .push(DnType::CommonName, "Rotating Proxy CA");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Gerami Proxy");

    // Valid for 10 years
    let not_before = rcgen::date_time_ymd(2024, 1, 1);
    let not_after = rcgen::date_time_ymd(2034, 1, 1);
    params.not_before = not_before;
    params.not_after = not_after;

    let cert = params
        .self_signed(&key_pair)
        .wrap_err("Failed to self-sign CA certificate")?;

    Ok((cert.pem(), key_pair.serialize_pem()))
}

fn build_authority(cert_pem: &str, key_pem: &str) -> crate::Result<RcgenAuthority> {
    let key_pair = KeyPair::from_pem(key_pem).wrap_err("Failed to parse CA private key")?;
    let issuer =
        Issuer::from_ca_cert_pem(cert_pem, key_pair).wrap_err("Failed to parse CA certificate")?;

    Ok(RcgenAuthority::new(
        issuer,
        CERT_CACHE_SIZE,
        aws_lc_rs::default_provider(),
    ))
}
