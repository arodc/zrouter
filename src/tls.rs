use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// Build a server-side TLS config. Returns `None` when TLS is disabled.
pub fn build_server_config(
    cfg: &crate::config::ServerConfig,
    config_path: &Path,
) -> Result<Option<Arc<rustls::ServerConfig>>, Box<dyn std::error::Error + Send + Sync>> {
    if !cfg.tls {
        return Ok(None);
    }

    let (cert_chain, key_der) = match (&cfg.cert_file, &cfg.key_file) {
        (Some(c), Some(k)) => load_pem_cert_key(c, k)?,
        _ => {
            let dev_dir = config_path.parent().unwrap_or(Path::new("."));
            let cert_path = dev_dir.join("zrouter-dev-cert.pem");
            let key_path = dev_dir.join("zrouter-dev-key.pem");
            load_or_generate_dev_cert(&cert_path, &key_path)?
        }
    };

    let provider = rustls::crypto::ring::default_provider();
    let mut config = rustls::ServerConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .expect("inconsistent TLS version config")
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .map_err(|e| format!("Invalid certificate/key: {}", e))?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(Some(Arc::new(config)))
}

fn load_or_generate_dev_cert(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), Box<dyn std::error::Error + Send + Sync>> {
    if cert_path.exists() && key_path.exists() {
        match load_pem_cert_key(cert_path.to_str().unwrap_or(""), key_path.to_str().unwrap_or("")) {
            Ok(pair) => {
                tracing::info!("Loaded persisted dev certificate from {}", cert_path.display());
                return Ok(pair);
            }
            Err(e) => {
                tracing::warn!("Failed to load persisted dev cert ({}), regenerating", e);
            }
        }
    }

    tracing::info!("Generating self-signed dev certificate");
    generate_and_save_dev_cert(cert_path, key_path)
}

fn generate_and_save_dev_cert(
    cert_path: &Path,
    key_path: &Path,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), Box<dyn std::error::Error + Send + Sync>> {
    use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};

    let key_pair = KeyPair::generate().map_err(|e| format!("Key generation failed: {}", e))?;

    let mut params = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "zrouter-dev");
    dn.push(DnType::OrganizationName, "zrouter");
    params.distinguished_name = dn;
    params.subject_alt_names = vec![
        SanType::DnsName("localhost".try_into().unwrap()),
        SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))),
        SanType::IpAddress(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)),
    ];

    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("Self-sign failed: {}", e))?;

    let saved = save_dev_cert_files(cert_path, key_path, &cert.pem(), &key_pair.serialize_pem());

    if saved {
        tracing::info!(
            "Dev certificate saved to {} — import into trust store: trust anchor {}",
            cert_path.display(),
            cert_path.display()
        );
    }

    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();

    Ok((
        vec![CertificateDer::from(cert_der)],
        PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key_der)),
    ))
}

fn save_dev_cert_files(cert_path: &Path, key_path: &Path, cert_pem: &str, key_pem: &str) -> bool {
    if let Some(dir) = cert_path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
    }
    let cert_ok = std::fs::write(cert_path, cert_pem).is_ok();
    let key_ok = std::fs::write(key_path, key_pem).is_ok();

    #[cfg(unix)]
    if key_ok {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(key_path, std::fs::Permissions::from_mode(0o600));
    }

    cert_ok && key_ok
}

fn load_pem_cert_key(
    cert_path: &str,
    key_path: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), Box<dyn std::error::Error + Send + Sync>> {
    let cert_pem = std::fs::read(cert_path)
        .map_err(|e| format!("Failed to read cert file '{}': {}", cert_path, e))?;
    let key_pem = std::fs::read(key_path)
        .map_err(|e| format!("Failed to read key file '{}': {}", key_path, e))?;

    let cert_chain: Vec<_> = rustls_pemfile::certs(&mut &cert_pem[..])
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to parse cert PEM: {}", e))?;

    if cert_chain.is_empty() {
        return Err("No certificates found in cert file".into());
    }

    let key_der = rustls_pemfile::private_key(&mut &key_pem[..])
        .map_err(|e| format!("Failed to parse key PEM: {}", e))?
        .ok_or("No private key found in key file")?;

    Ok((cert_chain, key_der))
}
