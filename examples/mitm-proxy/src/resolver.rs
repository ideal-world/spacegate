use std::net::IpAddr;
use std::sync::OnceLock;
use std::{collections::HashMap, sync::Arc};

use openssl::stack::Stack;
use openssl::x509::extension::SubjectAlternativeName;
use openssl::{
    asn1::Asn1Time,
    bn::BigNum,
    ec::{EcGroup, EcKey},
    hash::MessageDigest,
    pkey::PKey,
    x509::{X509Builder, X509Req, X509},
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    server::{ClientHello, ResolvesServerCert},
    sign::{CertifiedKey, SigningKey},
};
#[derive(Clone, Debug)]
pub struct MitmCertResolver {
    domain: Arc<str>,
    resolver: Arc<DynamicCertResolver>,
}

impl MitmCertResolver {
    pub fn new(domain: impl Into<Arc<str>>) -> Self {
        let domain = domain.into();
        let resolver = DynamicCertResolver::global();
        resolver.register_if_not_exists(&domain);
        Self { domain, resolver }
    }
}

impl ResolvesServerCert for MitmCertResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<rustls::sign::CertifiedKey>> {
        if let Some(domain) = client_hello.server_name() {
            Some(self.resolver.ensure_and_get(domain))
        } else {
            Some(self.resolver.ensure_and_get(&self.domain))
        }
    }
}

#[derive(Clone, Debug)]
pub struct DynamicCertResolver {
    pub generator: Arc<DomainCkGenerator>,
    pub repository: Arc<DomainCertRepository>,
}

impl DynamicCertResolver {
    pub fn global() -> Arc<Self> {
        static GLOBAL: OnceLock<Arc<DynamicCertResolver>> = OnceLock::new();
        GLOBAL.get_or_init(|| Arc::new(DynamicCertResolver::from_env())).clone()
    }
    pub fn register_if_not_exists(&self, domain: &str) {
        if self.repository.get_cert(domain).is_none() {
            self.register(domain);
        }
    }
    pub fn ensure_and_get(&self, domain: &str) -> Arc<rustls::sign::CertifiedKey> {
        if let Some(cert) = self.repository.get_cert(domain) {
            cert
        } else {
            self.register(domain);
            self.repository.get_cert(domain).unwrap()
        }
    }
    pub fn register(&self, domain: &str) {
        let csr = if let Ok(_ip) = domain.parse::<IpAddr>() {
            tracing::info!("Generate cert for ip: {}", domain);
            self.generator.build_request_with_ip(domain).unwrap()
        } else {
            tracing::info!("Generate cert for domain: {}", domain);
            self.generator.build_request(domain).unwrap()
        };
        let signed_cert = self.generator.sign_csr_with_ca(&csr).unwrap();
        let signed_cert_pem = String::from_utf8(signed_cert.to_pem().unwrap()).unwrap();
        tracing::info!("Generated cert for domain: {}", domain);
        tracing::debug!("Generated cert: \n{}", signed_cert_pem);
        let domain_cert = convert_openssl_cert_to_rustls_cert(&signed_cert).unwrap();
        let ca_cert = convert_openssl_cert_to_rustls_cert(&self.generator.ca_cert).unwrap();
        let key = convert_openssl_key_to_rustls_signing_key(&self.generator.cert_pkey).unwrap();
        let cert = CertifiedKey::new(vec![domain_cert, ca_cert], key);
        self.repository.insert_cert(domain, cert.clone());
    }
    pub fn from_env() -> Self {
        let args = crate::clap::args();
        let cert_bytes = std::fs::read(&args.cert).expect("Failed to read cert file");
        let key_bytes = std::fs::read(&args.key).expect("Failed to read key file");
        let ca_cert = X509::from_pem(&cert_bytes).unwrap();
        let ca_key = PKey::private_key_from_pem(&key_bytes).unwrap();
        let ec_group = EcGroup::from_curve_name(openssl::nid::Nid::X9_62_PRIME256V1).expect("Failed to create EC group");
        let ec_pkey = EcKey::generate(ec_group.as_ref()).expect("Failed to generate EC key");
        let cert_pkey = PKey::from_ec_key(ec_pkey).expect("Failed to create PKey from EC key");
        let generator = Arc::new(DomainCkGenerator { ca_cert, ca_key, cert_pkey });
        let repository = Arc::new(DomainCertRepository {
            signed_certs: Arc::new(std::sync::RwLock::new(HashMap::new())),
        });
        Self { generator, repository }
    }
}

#[derive(Debug)]
pub struct DomainCertRepository {
    signed_certs: Arc<std::sync::RwLock<HashMap<String, Arc<CertifiedKey>>>>,
}

impl DomainCertRepository {
    pub fn get_cert(&self, domain: &str) -> Option<Arc<CertifiedKey>> {
        self.signed_certs.read().unwrap().get(domain).cloned()
    }
    pub fn insert_cert(&self, domain: &str, cert: CertifiedKey) {
        self.signed_certs.write().unwrap().insert(domain.to_string(), cert.into());
    }
}

#[derive(Debug)]
pub struct DomainCkGenerator {
    ca_cert: X509,
    ca_key: PKey<openssl::pkey::Private>,
    cert_pkey: PKey<openssl::pkey::Private>,
}

impl DomainCkGenerator {
    pub fn build_request(&self, domain: &str) -> Result<X509Req, Box<dyn std::error::Error>> {
        let mut req_builder = X509Req::builder()?;
        req_builder.set_pubkey(&self.cert_pkey)?;
        let mut name_builder = openssl::x509::X509NameBuilder::new()?;
        name_builder.append_entry_by_text("CN", domain)?;
        let name = name_builder.build();
        req_builder.set_subject_name(&name)?;
        req_builder.sign(&self.cert_pkey, MessageDigest::sha256())?;
        let csr: X509Req = req_builder.build();
        Ok(csr)
    }
    pub fn build_request_with_ip(&self, ip_addr: &str) -> Result<X509Req, Box<dyn std::error::Error>> {
        let mut req_builder = X509Req::builder()?;
        req_builder.set_pubkey(&self.cert_pkey)?;
        let name_builder = openssl::x509::X509NameBuilder::new()?;
        let name = name_builder.build();
        req_builder.set_subject_name(&name)?;
        req_builder.sign(&self.cert_pkey, MessageDigest::sha256())?;
        let san = SubjectAlternativeName::new().ip(ip_addr).build(&req_builder.x509v3_context(None))?;
        let mut extensions = Stack::new()?;
        extensions.push(san)?;
        req_builder.add_extensions(&extensions)?;
        let csr = req_builder.build();
        Ok(csr)
    }
    pub fn sign_csr_with_ca(&self, csr: &X509Req) -> Result<X509, Box<dyn std::error::Error>> {
        let mut builder = X509Builder::new()?;

        // 设置证书的公钥
        builder.set_pubkey(csr.public_key()?.as_ref())?;

        // 设置证书主题和签发者
        builder.set_subject_name(csr.subject_name())?;
        builder.set_issuer_name(self.ca_cert.subject_name())?;
        // 设置证书有效期
        builder.set_not_before(Asn1Time::days_from_now(0)?.as_ref())?;
        builder.set_not_after(Asn1Time::days_from_now(365)?.as_ref())?;

        // 设置序列号
        let mut serial = BigNum::new()?;
        serial.rand(159, openssl::bn::MsbOption::MAYBE_ZERO, false)?;
        let serial = serial.to_asn1_integer()?;
        builder.set_serial_number(&serial)?;
        if let Ok(extensions) = csr.extensions() {
            for ext in extensions {
                builder.append_extension(ext)?;
            }
        }
        // 使用 CA 的私钥签署证书
        builder.sign(&self.ca_key, MessageDigest::sha256())?;

        let signed_cert = builder.build();
        Ok(signed_cert)
    }
}

fn convert_openssl_key_to_rustls_signing_key(pkey: &PKey<openssl::pkey::Private>) -> Result<Arc<dyn SigningKey>, Box<dyn std::error::Error>> {
    // 将 OpenSSL 私钥导出为 DER 格式
    let key_der = pkey.private_key_to_der()?;
    let key_der = PrivateKeyDer::try_from(key_der.as_slice())?;
    // 判断私钥类型并转换为 rustls 的 SigningKey
    #[allow(clippy::if_same_then_else)]
    if pkey.rsa().is_ok() {
        let rsa_signer = rustls::crypto::ring::sign::any_supported_type(&key_der)?;
        Ok(rsa_signer)
    } else if pkey.ec_key().is_ok() {
        let ecdsa_signer = rustls::crypto::ring::sign::any_supported_type(&key_der)?;
        Ok(ecdsa_signer)
    } else {
        Err("Unsupported key type".into())
    }
}

fn convert_openssl_cert_to_rustls_cert(cert: &X509) -> Result<CertificateDer<'static>, Box<dyn std::error::Error>> {
    // 将证书转换为 DER 格式
    let cert_der = cert.to_der()?;
    Ok(CertificateDer::from(cert_der).into_owned())
}
