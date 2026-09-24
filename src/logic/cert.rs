//! X.509 certificates: PEM blocks (one or a chain) or bare Base64 DER.

use base64::Engine as _;
use x509_parser::extensions::GeneralName;
use x509_parser::objects::{oid2sn, oid_registry};
use x509_parser::pem::Pem;
use x509_parser::prelude::{FromDer, X509Certificate};
use x509_parser::public_key::PublicKey;

pub struct CertInfo {
    /// The subject's common name, or the whole subject.
    pub title: String,
    pub rows: Vec<(&'static str, String)>,
    /// Whole days until expiry; negative once expired.
    pub days_left: i64,
    pub not_yet_valid: bool,
}

fn fingerprint<D: sha2::Digest>(der: &[u8]) -> String {
    D::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
}

fn oid_name(oid: &x509_parser::der_parser::oid::Oid) -> String {
    oid2sn(oid, oid_registry()).map(str::to_string).unwrap_or_else(|_| oid.to_id_string())
}

fn date(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0).map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string()).unwrap_or_default()
}

fn describe(der: &[u8]) -> Result<CertInfo, String> {
    let (_, cert) = X509Certificate::from_der(der).map_err(|e| format!("Not a valid X.509 certificate: {e}"))?;
    let subject = cert.subject();
    let title = subject
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| subject.to_string());
    let v = cert.validity();
    let (from, to) = (v.not_before.timestamp(), v.not_after.timestamp());
    let now = chrono::Utc::now().timestamp();
    let days_left = (to - now).div_euclid(86_400);

    let mut rows: Vec<(&'static str, String)> = vec![
        ("Subject", subject.to_string()),
        ("Issuer", cert.issuer().to_string()),
        ("Valid from", date(from)),
        ("Valid until", date(to)),
    ];
    if let Ok(Some(san)) = cert.subject_alternative_name() {
        let names: Vec<String> = san
            .value
            .general_names
            .iter()
            .map(|n| match n {
                GeneralName::DNSName(d) => d.to_string(),
                GeneralName::RFC822Name(e) => e.to_string(),
                GeneralName::URI(u) => u.to_string(),
                GeneralName::IPAddress(b) => match b.len() {
                    4 => std::net::Ipv4Addr::from(<[u8; 4]>::try_from(*b).unwrap()).to_string(),
                    16 => std::net::Ipv6Addr::from(<[u8; 16]>::try_from(*b).unwrap()).to_string(),
                    _ => format!("{b:?}"),
                },
                other => format!("{other:?}"),
            })
            .collect();
        rows.push(("Alternative names", names.join(", ")));
    }
    let spki = cert.public_key();
    let key = match spki.parsed() {
        Ok(PublicKey::RSA(k)) => format!("RSA {} bits", k.key_size()),
        Ok(PublicKey::EC(k)) => {
            let curve = spki.algorithm.parameters.as_ref().and_then(|p| p.as_oid().ok()).map(|o| oid_name(&o)).unwrap_or_default();
            format!("EC {} bits {curve}", k.key_size()).trim_end().to_string()
        }
        _ => oid_name(&spki.algorithm.algorithm),
    };
    rows.push(("Public key", key));
    rows.push(("Signature", oid_name(&cert.signature_algorithm.algorithm)));
    rows.push(("Serial", cert.raw_serial_as_string().to_uppercase()));
    rows.push(("Version", format!("{}", cert.version().0 + 1)));
    rows.push(("Certificate authority", if cert.is_ca() { "Yes".into() } else { "No".into() }));
    if let Ok(Some(ku)) = cert.key_usage() {
        rows.push(("Key usage", ku.value.to_string()));
    }
    if let Ok(Some(eku)) = cert.extended_key_usage() {
        let e = eku.value;
        let mut uses: Vec<String> = [
            (e.server_auth, "TLS server"),
            (e.client_auth, "TLS client"),
            (e.code_signing, "Code signing"),
            (e.email_protection, "Email"),
            (e.time_stamping, "Time stamping"),
            (e.ocsp_signing, "OCSP signing"),
            (e.any, "Any"),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| n.to_string())
        .collect();
        uses.extend(e.other.iter().map(oid_name));
        rows.push(("Extended key usage", uses.join(", ")));
    }
    rows.push(("SHA-256 fingerprint", fingerprint::<sha2::Sha256>(der)));
    rows.push(("SHA-1 fingerprint", {
        use sha1::Digest as _;
        sha1::Sha1::digest(der).iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":")
    }));
    Ok(CertInfo { title, rows, days_left, not_yet_valid: from > now })
}

/// Every certificate in the input, in order.
pub fn decode(input: &str) -> Result<Vec<CertInfo>, String> {
    let t = input.trim();
    if t.contains("-----BEGIN") {
        let mut out = Vec::new();
        for pem in Pem::iter_from_buffer(t.as_bytes()) {
            let pem = pem.map_err(|e| format!("Invalid PEM block: {e}"))?;
            if pem.label.contains("CERTIFICATE") {
                out.push(describe(&pem.contents)?);
            } else if pem.label.contains("PRIVATE KEY") {
                return Err("This is a private key, not a certificate. Don't paste private keys into tools you don't trust.".into());
            }
        }
        if out.is_empty() {
            return Err("No CERTIFICATE block was found".into());
        }
        return Ok(out);
    }
    let clean: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    let der = base64::engine::general_purpose::STANDARD.decode(clean).map_err(|_| "Paste a PEM certificate (-----BEGIN CERTIFICATE-----) or Base64 DER".to_string())?;
    Ok(vec![describe(&der)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    const PEM: &str = "-----BEGIN CERTIFICATE-----
MIIB2TCCAYCgAwIBAgIUT+smGBqk9DX+i3hgl5lA/zaqMiMwCgYIKoZIzj0EAwIw
KTEVMBMGA1UEAwwMc2lkZWtpdC50ZXN0MRAwDgYDVQQKDAdTaWRlS2l0MB4XDTI2
MDkyNDE4MzkzNVoXDTM2MDkyMTE4MzkzNVowKTEVMBMGA1UEAwwMc2lkZWtpdC50
ZXN0MRAwDgYDVQQKDAdTaWRlS2l0MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE
o8Fg+CSac5RXxTMoiTldThP42mIW2mJi7JL1dComWlko+ECLlXZEFtGXizr8POgj
GWi3ReBkZR8bY8bQuioW4aOBhTCBgjAdBgNVHQ4EFgQUq1ptnmODM4kkTX8EXRQd
ku/rNBEwHwYDVR0jBBgwFoAUq1ptnmODM4kkTX8EXRQdku/rNBEwDwYDVR0TAQH/
BAUwAwEB/zAvBgNVHREEKDAmggxzaWRla2l0LnRlc3SCEHd3dy5zaWRla2l0LnRl
c3SHBH8AAAEwCgYIKoZIzj0EAwIDRwAwRAIgUq7tFFWcJV+3FIjrjw8kQqsqsi16
yTBGQ3WdJQ52h78CIGMOo6rASsM/WkZU0qLIKkt7IehRRgRaWVlq2Q30vqFO
-----END CERTIFICATE-----";

    #[test]
    fn decodes_pem_and_der() {
        let certs = decode(PEM).unwrap();
        assert_eq!(certs.len(), 1);
        let c = &certs[0];
        assert_eq!(c.title, "sidekit.test");
        let get = |k: &str| c.rows.iter().find(|r| r.0 == k).unwrap().1.clone();
        assert_eq!(get("Alternative names"), "sidekit.test, www.sidekit.test, 127.0.0.1");
        assert_eq!(get("Serial"), "4F:EB:26:18:1A:A4:F4:35:FE:8B:78:60:97:99:40:FF:36:AA:32:23");
        assert!(get("SHA-256 fingerprint").starts_with("50:A1:50:48:C1:5D"));
        assert!(get("Public key").starts_with("EC 256 bits"));
        assert_eq!(get("Valid until"), "2036-09-21 18:39:35 UTC");
        assert_eq!(get("Certificate authority"), "Yes");

        let body: String = PEM.lines().filter(|l| !l.starts_with("-----")).collect();
        assert_eq!(decode(&body).unwrap()[0].title, "sidekit.test");
        let chain = format!("{PEM}\n{PEM}");
        assert_eq!(decode(&chain).unwrap().len(), 2);
        assert!(decode("hello").is_err());
        assert!(decode("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----").is_err());
    }
}
